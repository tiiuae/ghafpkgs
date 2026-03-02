/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include "proxy.h"
#include "gdbusprivate.h"
#include "log.h"
#include <glib/gprintf.h>
#include <gtk/gtk.h>
#include <unistd.h>

// Proxied D-Bus object with its registered interfaces (local to SNI proxy)
struct SniProxiedObject {
    std::string object_path;
    GNodeInfoPtr node_info;
    GHashTable* registration_ids = nullptr; // interface_name -> registration_id

    SniProxiedObject(const char* path, GDBusNodeInfo* info)
        : object_path(path),
          registration_ids(g_hash_table_new_full(g_str_hash, g_str_equal, g_free, nullptr)) {
        node_info.reset(g_dbus_node_info_ref(info));
    }

    ~SniProxiedObject() {
        if (registration_ids)
            g_hash_table_destroy(registration_ids);
    }
};

// --- Static data ---

const GDBusInterfaceVTable SniProxy::item_vtable_ = {.method_call = SniProxy::on_method_call,
                                                     .get_property = SniProxy::on_get_property,
                                                     .set_property = SniProxy::on_set_property,
                                                     .padding = {nullptr}};

const GDBusInterfaceVTable SniProxy::watcher_vtable_ = {
    .method_call = SniProxy::on_watcher_method_call,
    .get_property = SniProxy::on_watcher_get_property,
    .set_property = nullptr,
    .padding = {nullptr}};

// --- Helper: async method call context ---

struct SniMethodCallContext {
    GDBusMethodInvocation* invocation = nullptr; // owned (caller must g_object_ref before storing)
    std::string forward_bus_name;
    ~SniMethodCallContext() {
        if (invocation)
            g_object_unref(invocation);
    }
};

// --- XDG activation workaround for dbusmenu.Event ---
// Electron/Chromium apps (Element, Teams, Discord) require an XDG activation
// token before processing dbusmenu.Event on Wayland. Without it, Electron's
// window.focus() throws "error occurred in Event" because the compositor
// rejects focus requests with no activation token.
//
// Fix: get a real token from the Wayland compositor via GDK, send it via
// ProvideXdgActivationToken, then forward the Event. This replaces the previous
// Activate-before-Event hack (which caused Show/Hide to toggle in the wrong
// direction by opening the window before the toggle event arrived).

struct SniDeferredEventCtx {
    GDBusConnection* source_bus; // borrowed — do not unref
    std::string source_bus_name;
    std::string dbusmenu_path;
    GVariant* event_params;            // owned — caller must pass a ref'd GVariant*
    GDBusMethodInvocation* invocation; // borrowed/nullptr — do not unref

    SniDeferredEventCtx(GDBusConnection* bus, std::string bus_name, std::string menu_path,
                        GVariant* owned_params, GDBusMethodInvocation* inv)
        : source_bus(bus), source_bus_name(std::move(bus_name)),
          dbusmenu_path(std::move(menu_path)), event_params(owned_params), invocation(inv) {}

    ~SniDeferredEventCtx() {
        if (event_params)
            g_variant_unref(event_params);
    }
};

// Forward the deferred dbusmenu.Event to the source app.
// If ctx->invocation is non-null the panel is still waiting for a reply and we
// use method_reply_callback to close the loop.  When it is null the panel
// already received an immediate OK, so we fire-and-forget to the app.
static void forward_deferred_event(std::unique_ptr<SniDeferredEventCtx> ctx) {
    Log::info() << "[XDG] Forwarding dbusmenu.Event to " << ctx->source_bus_name.c_str();

    if (ctx->invocation) {
        auto call_ctx = std::make_unique<SniMethodCallContext>();
        call_ctx->invocation = ctx->invocation;
        g_object_ref(ctx->invocation); // call_ctx owns a ref
        call_ctx->forward_bus_name = ctx->source_bus_name;
        g_dbus_connection_call(ctx->source_bus, ctx->source_bus_name.c_str(),
                               ctx->dbusmenu_path.c_str(), DBUSMENU_INTERFACE, "Event",
                               ctx->event_params, nullptr, G_DBUS_CALL_FLAGS_NONE, 10000, nullptr,
                               SniProxy::method_reply_callback, call_ctx.release());
    } else {
        // Panel already got OK — fire-and-forget.
        g_dbus_connection_call(ctx->source_bus, ctx->source_bus_name.c_str(),
                               ctx->dbusmenu_path.c_str(), DBUSMENU_INTERFACE, "Event",
                               ctx->event_params, nullptr, G_DBUS_CALL_FLAGS_NONE, 10000, nullptr,
                               nullptr, nullptr);
    }
    // ctx destructor unrefs event_params
}

// Timer callback: COSMIC's popup is closed; forward Event directly.
// Calling Activate(0,0) before the Event was tried but caused Electron apps
// (Element, Teams) to schedule an async show() that fired after the Event's
// hide(), re-showing the window.  A plain delay is sufficient — once the popup
// releases exclusive compositor focus, Electron can process hide/show normally.
static gboolean on_event_click_delay(gpointer user_data) {
    auto ctx = std::unique_ptr<SniDeferredEventCtx>(static_cast<SniDeferredEventCtx*>(user_data));
    Log::info() << "[XDG] Popup-close delay elapsed for " << ctx->source_bus_name.c_str()
                << ": forwarding Event";
    forward_deferred_event(std::move(ctx));
    return G_SOURCE_REMOVE;
}

// --- SniItem destructor ---
// Called via GHashTable destroy callback (delete) or directly.
// Assumes signal unsubscription from source_bus already happened
// (done in remove_item() / ~SniProxy() before hash table destruction).

SniItem::~SniItem() {
    // Unregister all D-Bus objects before closing connection
    if (registered_objects && target_conn) {
        GHashTableIter iter;
        gpointer key;
        g_hash_table_iter_init(&iter, registered_objects);
        while (g_hash_table_iter_next(&iter, &key, nullptr))
            g_dbus_connection_unregister_object(target_conn.get(), GPOINTER_TO_UINT(key));
    }

    // Unown target bus name
    if (target_name_owner_id) {
        g_bus_unown_name(target_name_owner_id);
        target_name_owner_id = 0;
    }

    // Destroy hash tables
    if (proxied_objects)
        g_hash_table_destroy(proxied_objects);
    if (registered_objects)
        g_hash_table_destroy(registered_objects);
    if (node_info_cache)
        g_hash_table_destroy(node_info_cache);

    // target_conn GConnPtr destructor closes + unrefs
}

// --- Constructor / Destructor ---

SniProxy::SniProxy(GDBusConnection* source_bus, GDBusConnection* target_bus,
                   GBusType target_bus_type)
    : source_bus_(source_bus), target_bus_(target_bus), target_bus_type_(target_bus_type) {
    // Initialize GTK/GDK for XDG activation token generation.
    // The proxy runs in GUI-VM which has WAYLAND_DISPLAY. Tokens allow Electron
    // apps to process dbusmenu.Event (Show/Hide, Quit) without throwing
    // "error occurred in Event" due to missing window focus.
    // gtk_init() must be called before any GDK display operations.
    if (getenv("WAYLAND_DISPLAY")) {
        g_setenv("GDK_BACKEND", "wayland", FALSE);
        if (gtk_init_check()) {
            wayland_display_ = gdk_display_get_default();
            if (wayland_display_) {
                Log::info() << "GTK/GDK initialized on Wayland for XDG activation tokens";
            } else {
                Log::info() << "gtk_init_check succeeded but no default display";
            }
        } else {
            Log::info() << "gtk_init_check failed - XDG activation unavailable";
        }
    } else {
        Log::info() << "WAYLAND_DISPLAY not set - XDG activation unavailable";
    }

    Log::info() << "Initializing SNI proxy";

    sni_items_ = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                       [](gpointer p) { delete static_cast<SniItem*>(p); });
    registered_items_ = g_ptr_array_new_with_free_func(g_free);
    pending_items_ = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, g_free);

    // We are the host from source apps' perspective
    host_registered_ = TRUE;

    // Register watcher object on source bus (so apps on source side can register)
    if (!register_watcher())
        return;

    // Subscribe to NameOwnerChanged BEFORE initial scan (race condition
    // prevention)
    if (!subscribe_name_owner_changed()) {
        Log::error() << "Failed to subscribe to NameOwnerChanged";
        return;
    }

    // Own watcher name (triggers initial_scan via callback)
    if (!own_watcher_name())
        return;

    Log::info() << "SNI proxy initialized -- waiting for items";
    initialized_ = true;
}

std::unique_ptr<SniProxy> SniProxy::create(GDBusConnection* source_bus, GDBusConnection* target_bus,
                                           GBusType target_bus_type) {
    auto proxy = std::make_unique<SniProxy>(source_bus, target_bus, target_bus_type);
    if (!proxy->initialized_)
        return nullptr;
    return proxy;
}

// Lazy Wayland init: called on first dbusmenu.Event when the proxy runs as a
// system service without WAYLAND_DISPLAY in its environment. Scans
// /run/user/<uid>/ for a wayland-* socket and initialises GTK/GDK if found.
void SniProxy::try_lazy_wayland_init() {
    if (wayland_display_ || wayland_init_tried_)
        return;
    wayland_init_tried_ = TRUE;

    if (!getenv("WAYLAND_DISPLAY")) {
        uid_t uid = getuid();
        g_autofree gchar* runtime_dir = g_strdup_printf("/run/user/%u", (unsigned)uid);

        g_autoptr(GError) dir_err = nullptr;
        GDir* dir = g_dir_open(runtime_dir, 0, &dir_err);
        if (dir) {
            const gchar* entry;
            while ((entry = g_dir_read_name(dir)) != nullptr) {
                // Match "wayland-N" but skip ".lock" files
                if (g_str_has_prefix(entry, "wayland-") && !g_str_has_suffix(entry, ".lock")) {
                    const std::string socket_path = std::string(runtime_dir) + "/" + entry;
                    // Verify it is actually a socket (not a directory)
                    if (g_file_test(socket_path.c_str(), G_FILE_TEST_EXISTS)) {
                        g_setenv("XDG_RUNTIME_DIR", runtime_dir, FALSE);
                        g_setenv("WAYLAND_DISPLAY", entry, FALSE);
                        Log::info() << "[XDG] Auto-detected Wayland socket: " << socket_path;
                        break;
                    }
                }
            }
            g_dir_close(dir);
        } else {
            Log::info() << "[XDG] Cannot open " << runtime_dir << ": "
                        << (dir_err ? dir_err->message : "unknown");
        }
    }

    if (!getenv("WAYLAND_DISPLAY")) {
        Log::info() << "[XDG] Lazy init: no Wayland socket found, XDG activation ";
        return;
    }

    g_setenv("GDK_BACKEND", "wayland", FALSE);
    if (gtk_init_check()) {
        wayland_display_ = gdk_display_get_default();
        if (wayland_display_)
            Log::info() << "[XDG] Lazy GTK/GDK initialized for XDG activation tokens";
        else
            Log::info() << "[XDG] Lazy gtk_init_check OK but no default display";
    } else {
        Log::info() << "[XDG] Lazy gtk_init_check failed";
    }
}

SniProxy::~SniProxy() {
    // wayland_display_ is the GDK default display managed by GTK — do not close
    wayland_display_ = nullptr;

    // Unsubscribe NameOwnerChanged
    if (name_owner_changed_sub_ && source_bus_)
        g_dbus_connection_signal_unsubscribe(source_bus_, name_owner_changed_sub_);

    // Unsubscribe per-item signals from source bus (needs source_bus_, which
    // ~SniItem() doesn't have), then destroy hash table (triggers ~SniItem()
    // for each item which handles object unregister + name unown + conn close).
    if (sni_items_) {
        GHashTableIter iter;
        gpointer key, value;
        g_hash_table_iter_init(&iter, sni_items_);
        while (g_hash_table_iter_next(&iter, &key, &value)) {
            SniItem* item = static_cast<SniItem*>(value);
            if (item->signal_subscription_id && source_bus_) {
                g_dbus_connection_signal_unsubscribe(source_bus_, item->signal_subscription_id);
                item->signal_subscription_id = 0;
            }
        }
        g_hash_table_destroy(sni_items_);
        sni_items_ = nullptr;
    }

    // Unregister watcher from source bus
    if (watcher_reg_id_ && source_bus_)
        g_dbus_connection_unregister_object(source_bus_, watcher_reg_id_);

    // Release watcher name
    if (watcher_name_owner_id_)
        g_bus_unown_name(watcher_name_owner_id_);

    if (registered_items_)
        g_ptr_array_free(registered_items_, TRUE);

    if (pending_items_)
        g_hash_table_destroy(pending_items_);

    // watcher_node_info_ GNodeInfoPtr destructor calls g_dbus_node_info_unref
}

// --- Name resolution: find well-known SNI name owned by a unique name ---
// Scans ListNames for org.kde.StatusNotifierItem-* names, then checks
// which one is owned by the given unique name via GetNameOwner.
// Returns newly allocated string or nullptr.

char* SniProxy::resolve_unique_to_wellknown(const char* unique_name) {
    Log::info() << "[CALL] ListNames -> org.freedesktop.DBus";
    g_autoptr(GError) error = nullptr;
    g_autoptr(GVariant) result = g_dbus_connection_call_sync(
        source_bus_, DBUS_INTERFACE_DBUS, DBUS_OBJECT_PATH_DBUS, DBUS_INTERFACE_DBUS, "ListNames",
        nullptr, G_VARIANT_TYPE("(as)"), G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, &error);

    if (!result) {
        Log::error() << "[REPLY] ListNames FAILED: " << (error ? error->message : "Unknown");
        return nullptr;
    }

    // Log ALL names returned by ListNames
    GVariantIter* all_iter;
    const char* all_name;
    int total_count = 0;
    g_variant_get(result, "(as)", &all_iter);
    Log::info() << "[REPLY] ListNames returned names:";
    while (g_variant_iter_next(all_iter, "&s", &all_name)) {
        Log::info() << "  [" << total_count << "] " << all_name;
        total_count++;
    }
    g_variant_iter_free(all_iter);
    Log::info() << "[REPLY] ListNames total: " << total_count << " names";

    char* resolved = nullptr;
    GVariantIter* iter;
    const char* name;
    int sni_name_count = 0;
    g_variant_get(result, "(as)", &iter);
    while (g_variant_iter_next(iter, "&s", &name)) {
        if (!g_str_has_prefix(name, SNI_ITEM_BUS_NAME_PREFIX))
            continue;

        sni_name_count++;

        // Check if this well-known name is owned by the unique name
        Log::info() << "[CALL] GetNameOwner(" << name << ") -> org.freedesktop.DBus";
        g_autoptr(GError) owner_error = nullptr;
        g_autoptr(GVariant) owner_result = g_dbus_connection_call_sync(
            source_bus_, DBUS_INTERFACE_DBUS, DBUS_OBJECT_PATH_DBUS, DBUS_INTERFACE_DBUS,
            "GetNameOwner", g_variant_new("(s)", name), G_VARIANT_TYPE("(s)"),
            G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, &owner_error);

        if (owner_result) {
            const char* owner;
            g_variant_get(owner_result, "(&s)", &owner);
            Log::info() << "[REPLY] GetNameOwner(" << name << ") = " << owner << " (looking for "
                        << unique_name << ")";
            if (g_strcmp0(owner, unique_name) == 0) {
                resolved = g_strdup(name);
                break;
            }
        } else {
            Log::error() << "[REPLY] GetNameOwner(" << name
                         << ") FAILED: " << (owner_error ? owner_error->message : "Unknown");
        }
    }
    g_variant_iter_free(iter);

    if (sni_name_count == 0) {
        Log::info() << "ListNames returned NO org.kde.StatusNotifierItem-* names";
    } else {
        Log::info() << "ListNames returned " << sni_name_count << " SNI names";
    }

    if (resolved) {
        Log::info() << "Resolved " << unique_name << " -> " << resolved;
    } else {
        Log::info() << "No well-known SNI name found for " << unique_name;
    }
    return resolved;
}

// --- Watcher: method call handler ---

void SniProxy::on_watcher_method_call(GDBusConnection* connection, const char* sender,
                                      G_GNUC_UNUSED const char* object_path,
                                      G_GNUC_UNUSED const char* interface_name,
                                      const char* method_name, GVariant* parameters,
                                      GDBusMethodInvocation* invocation, gpointer user_data) {
    SniProxy& self = *static_cast<SniProxy*>(user_data);

    if (g_strcmp0(method_name, "RegisterStatusNotifierItem") == 0) {
        const char* service;
        g_variant_get(parameters, "(&s)", &service);
        Log::info() << "RegisterStatusNotifierItem from " << sender << ": " << service;

        // Determine bus name and object path from service parameter.
        if (service[0] == '/') {
            // service is an object path, use sender as bus name
            self.discover_and_proxy_item(sender, service);
        } else {
            // service is a bus name (unique or well-known)
            self.discover_and_proxy_item(service);
        }

        g_dbus_method_invocation_return_value(invocation, nullptr);

    } else if (g_strcmp0(method_name, "RegisterStatusNotifierHost") == 0) {
        const char* service;
        g_variant_get(parameters, "(&s)", &service);
        Log::info() << "RegisterStatusNotifierHost from " << sender << ": " << service;

        self.host_registered_ = TRUE;

        g_autoptr(GError) error = nullptr;
        g_dbus_connection_emit_signal(self.source_bus_, nullptr, SNI_WATCHER_OBJECT_PATH,
                                      SNI_WATCHER_INTERFACE, "StatusNotifierHostRegistered",
                                      nullptr, &error);
        if (error)
            Log::error() << "Failed to emit StatusNotifierHostRegistered: " << error->message;

        g_dbus_method_invocation_return_value(invocation, nullptr);

    } else {
        g_dbus_method_invocation_return_error(invocation, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_METHOD,
                                              "Unknown method: %s", method_name);
    }
}

// --- Watcher: property getter ---

GVariant* SniProxy::on_watcher_get_property(G_GNUC_UNUSED GDBusConnection* connection,
                                            const char* sender,
                                            G_GNUC_UNUSED const char* object_path,
                                            G_GNUC_UNUSED const char* interface_name,
                                            const char* property_name, GError** error,
                                            gpointer user_data) {
    SniProxy& self = *static_cast<SniProxy*>(user_data);

    Log::info() << "[WATCHER] Get property '" << property_name << "' from "
                << (sender ? sender : "(null)");

    if (g_strcmp0(property_name, "RegisteredStatusNotifierItems") == 0) {
        GVariantBuilder builder;
        g_variant_builder_init(&builder, G_VARIANT_TYPE("as"));
        if (self.registered_items_) {
            for (guint i = 0; i < self.registered_items_->len; i++) {
                const char* item_name =
                    static_cast<const char*>(g_ptr_array_index(self.registered_items_, i));
                Log::info() << "[WATCHER]   RegisteredItems[" << i << "] = " << item_name;
                g_variant_builder_add(&builder, "s", item_name);
            }
            Log::info() << "[WATCHER]   Total: " << self.registered_items_->len << " items";
        }
        return g_variant_builder_end(&builder);

    } else if (g_strcmp0(property_name, "IsStatusNotifierHostRegistered") == 0) {
        Log::info() << "[WATCHER]   = " << (self.host_registered_ ? "true" : "false");
        return g_variant_new_boolean(self.host_registered_);

    } else if (g_strcmp0(property_name, "ProtocolVersion") == 0) {
        Log::info() << "[WATCHER]   = 0";
        return g_variant_new_int32(0);
    }

    Log::error() << "[WATCHER] Unknown property: " << property_name;
    g_set_error(error, G_DBUS_ERROR, G_DBUS_ERROR_UNKNOWN_PROPERTY, "Unknown property: %s",
                property_name);
    return nullptr;
}

// --- Watcher: registration ---

gboolean SniProxy::register_watcher() {
    g_autoptr(GError) error = nullptr;

    watcher_node_info_.reset(g_dbus_node_info_new_for_xml(WATCHER_XML, &error));
    if (!watcher_node_info_) {
        Log::error() << "Failed to parse watcher XML: " << (error ? error->message : "Unknown");
        return FALSE;
    }

    watcher_reg_id_ = g_dbus_connection_register_object(
        source_bus_, SNI_WATCHER_OBJECT_PATH, watcher_node_info_->interfaces[0], &watcher_vtable_,
        this, // user_data = SniProxy*
        nullptr, &error);

    if (watcher_reg_id_ == 0) {
        Log::error() << "Failed to register watcher: " << (error ? error->message : "Unknown");
        return FALSE;
    }

    Log::info() << "Watcher registered at " << SNI_WATCHER_OBJECT_PATH;
    return TRUE;
}

// --- Watcher: name ownership callbacks ---

void SniProxy::on_watcher_name_acquired(G_GNUC_UNUSED GDBusConnection* conn, const gchar* name,
                                        gpointer user_data) {
    Log::info() << "Acquired watcher name: " << name;
    SniProxy& self = *static_cast<SniProxy*>(user_data);
    self.initial_scan();
}

void SniProxy::on_watcher_name_lost(G_GNUC_UNUSED GDBusConnection* conn, const gchar* name,
                                    G_GNUC_UNUSED gpointer user_data) {
    Log::error() << "Lost watcher name: " << name << " -- another watcher running?";
}

gboolean SniProxy::own_watcher_name() {
    watcher_name_owner_id_ = g_bus_own_name_on_connection(
        source_bus_, SNI_WATCHER_BUS_NAME, G_BUS_NAME_OWNER_FLAGS_REPLACE, on_watcher_name_acquired,
        on_watcher_name_lost,
        this, // user_data
        nullptr);

    if (watcher_name_owner_id_ == 0) {
        Log::error() << "Failed to request watcher bus name";
        return FALSE;
    }
    return TRUE;
}

// --- Per-item forwarding: method call ---

void SniProxy::method_reply_callback(GObject* source, GAsyncResult* res, gpointer user_data) {
    auto ctx = std::unique_ptr<SniMethodCallContext>(static_cast<SniMethodCallContext*>(user_data));
    g_autoptr(GError) error = nullptr;

    g_autoptr(GVariant) result =
        g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), res, &error);

    if (result) {
        g_autofree char* result_str = g_variant_print(result, TRUE);
        gboolean truncated = strlen(result_str) > 300;
        if (truncated)
            result_str[300] = '\0';
        Log::info() << "[REPLY] Method call to " << ctx->forward_bus_name.c_str()
                    << " OK: " << result_str << (truncated ? "..." : "");
        g_dbus_method_invocation_return_value(ctx->invocation, result);
    } else {
        Log::error() << "[REPLY] Method call to " << ctx->forward_bus_name.c_str()
                     << " FAILED: " << (error ? error->message : "Unknown");
        g_dbus_method_invocation_return_gerror(ctx->invocation, error);
    }

    // ctx unique_ptr destructor unrefs invocation
}

void SniProxy::on_method_call(G_GNUC_UNUSED GDBusConnection* connection,
                              G_GNUC_UNUSED const char* sender, const char* object_path,
                              const char* interface_name, const char* method_name,
                              GVariant* parameters, GDBusMethodInvocation* invocation,
                              gpointer user_data) {
    SniForwardContext& fwd = *static_cast<SniForwardContext*>(user_data);

    g_autofree char* params_str = parameters ? g_variant_print(parameters, TRUE) : g_strdup("()");
    Log::info() << "[CALL] " << interface_name << "." << method_name << params_str << " on "
                << object_path << " -> dest=" << fwd.source_bus_name.c_str()
                << " path=" << fwd.object_path.c_str();

    // dbusmenu.Event workaround for Electron apps (Element, Teams, Discord):
    //
    // For 'clicked' events we reply OK to the panel immediately so it can close
    // its popup without waiting for Element to respond.  Holding the invocation
    // open while delaying meant the popup stayed open for the whole delay period
    // (COSMIC waits for Event reply before dismissing the menu).
    // After a short delay (popup is now gone) we fire-and-forget the Event to
    // the app.
    //
    // 'opened', 'closed', etc. are forwarded directly — no delay needed.
    if (g_strcmp0(interface_name, DBUSMENU_INTERFACE) == 0 &&
        g_strcmp0(method_name, "Event") == 0) {

        // Extract event-type string (2nd field of (isvu) tuple).
        gchar* event_type = nullptr;
        if (parameters) {
            GVariant* tv = g_variant_get_child_value(parameters, 1);
            event_type = g_variant_dup_string(tv, nullptr);
            g_variant_unref(tv);
        }
        gboolean is_clicked = (g_strcmp0(event_type, "clicked") == 0);
        g_free(event_type);

        if (is_clicked) {
            // Reply to panel immediately — popup closes now, not after Element
            // responds.
            g_dbus_method_invocation_return_value(invocation, g_variant_new("()"));

            auto deferred = std::make_unique<SniDeferredEventCtx>(
                fwd.proxy.source_bus_, fwd.source_bus_name, fwd.object_path,
                parameters ? g_variant_ref(parameters) : nullptr,
                nullptr /* already replied above */);

            Log::info() << "[XDG] Event 'clicked' for " << fwd.source_bus_name.c_str()
                        << " — panel OK'd, forwarding after 50 ms";
            g_timeout_add(50, on_event_click_delay, deferred.release());
            return;
        }
        // 'opened', 'closed', etc. — fall through to normal forwarding.
    }

    auto ctx = std::make_unique<SniMethodCallContext>();
    ctx->invocation = invocation;
    g_object_ref(invocation); // ctx owns a ref via ~SniMethodCallContext()
    ctx->forward_bus_name = fwd.source_bus_name;

    g_dbus_connection_call(fwd.proxy.source_bus_, fwd.source_bus_name.c_str(),
                           fwd.object_path.c_str(), interface_name, method_name, parameters,
                           nullptr, G_DBUS_CALL_FLAGS_NONE, 10000, nullptr, method_reply_callback,
                           ctx.release());
}

// --- Per-item forwarding: property get ---

GVariant* SniProxy::on_get_property(G_GNUC_UNUSED GDBusConnection* connection,
                                    G_GNUC_UNUSED const char* sender, const char* object_path,
                                    const char* interface_name, const char* property_name,
                                    GError** error, gpointer user_data) {
    SniForwardContext& fwd = *static_cast<SniForwardContext*>(user_data);

    Log::info() << "[CALL] Get " << interface_name << "." << property_name << " on " << object_path
                << " -> dest=" << fwd.source_bus_name.c_str()
                << " path=" << fwd.object_path.c_str();

    g_autoptr(GVariant) result = g_dbus_connection_call_sync(
        fwd.proxy.source_bus_, fwd.source_bus_name.c_str(), fwd.object_path.c_str(),
        DBUS_INTERFACE_PROPERTIES, "Get", g_variant_new("(ss)", interface_name, property_name),
        G_VARIANT_TYPE("(v)"), G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, error);

    if (result) {
        GVariant* value;
        g_variant_get(result, "(v)", &value);
        g_autofree char* value_str = g_variant_print(value, TRUE);
        gboolean trunc = strlen(value_str) > 300;
        if (trunc)
            value_str[300] = '\0';
        Log::info() << "[REPLY] Get " << interface_name << "." << property_name << " = "
                    << value_str << (trunc ? "..." : "");
        return value;
    }

    Log::error() << "[REPLY] Get " << interface_name << "." << property_name << " FAILED on "
                 << fwd.object_path.c_str() << " (" << fwd.source_bus_name.c_str()
                 << "): " << ((error && *error) ? (*error)->message : "Unknown");
    return nullptr;
}

// --- Per-item forwarding: property set ---

gboolean SniProxy::on_set_property(G_GNUC_UNUSED GDBusConnection* connection,
                                   G_GNUC_UNUSED const char* sender,
                                   G_GNUC_UNUSED const char* object_path,
                                   const char* interface_name, const char* property_name,
                                   GVariant* value, GError** error, gpointer user_data) {
    SniForwardContext& fwd = *static_cast<SniForwardContext*>(user_data);

    g_autoptr(GVariant) result =
        g_dbus_connection_call_sync(fwd.proxy.source_bus_, fwd.source_bus_name.c_str(),
                                    fwd.object_path.c_str(), DBUS_INTERFACE_PROPERTIES, "Set",
                                    g_variant_new("(ssv)", interface_name, property_name, value),
                                    nullptr, G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, error);

    return result != nullptr;
}

// --- Per-item signal forwarding ---

void SniProxy::on_signal_received(G_GNUC_UNUSED GDBusConnection* connection,
                                  const char* sender_name, const char* object_path,
                                  const char* interface_name, const char* signal_name,
                                  GVariant* parameters, gpointer user_data) {
    SniProxy& self = *static_cast<SniProxy*>(user_data);
    // Find which item this signal belongs to
    SniItem* item = nullptr;
    GHashTableIter iter;
    gpointer key, value;
    g_hash_table_iter_init(&iter, self.sni_items_);
    while (g_hash_table_iter_next(&iter, &key, &value)) {
        SniItem* candidate = static_cast<SniItem*>(value);
        if (candidate->ready && g_hash_table_contains(candidate->proxied_objects, object_path)) {
            item = candidate;
            break;
        }
    }

    if (!item) {
        Log::verbose() << "[SIGNAL] " << interface_name << "." << signal_name << " at "
                       << object_path << " from " << sender_name << " ignored (not proxied)";
        return;
    }

    g_autofree char* params_str = parameters ? g_variant_print(parameters, TRUE) : g_strdup("()");
    Log::info() << "[SIGNAL] Forwarding " << interface_name << "." << signal_name << params_str
                << " at " << object_path << " from " << sender_name << " -> target bus";

    g_autoptr(GError) error = nullptr;
    g_dbus_connection_emit_signal(item->target_conn.get(), nullptr, object_path, interface_name,
                                  signal_name, parameters, &error);
    if (error) {
        Log::error() << "[SIGNAL] Failed to forward " << interface_name << "." << signal_name
                     << ": " << error->message;
    }
}

// --- Register a path using provided XML (no introspection needed) ---

gboolean SniProxy::register_path_with_xml(SniItem& item, const char* path, const char* xml) {
    if (g_hash_table_contains(item.proxied_objects, path))
        return TRUE;

    g_autoptr(GError) error = nullptr;
    g_autoptr(GDBusNodeInfo) node_info = g_dbus_node_info_new_for_xml(xml, &error);
    if (!node_info) {
        Log::error() << "Failed to parse hardcoded XML for " << path << ": "
                     << (error ? error->message : "Unknown");
        return FALSE;
    }

    auto proxied_obj = std::make_unique<SniProxiedObject>(path, node_info);

    guint registered_count = 0;

    if (node_info->interfaces) {
        for (int i = 0; node_info->interfaces[i]; i++) {
            GDBusInterfaceInfo* iface = node_info->interfaces[i];

            if (g_strv_contains(standard_interfaces_, iface->name))
                continue;

            auto ctx = std::make_unique<SniForwardContext>(path, item.source_bus_name, *this);

            guint reg_id = g_dbus_connection_register_object(
                item.target_conn.get(), path, iface, &item_vtable_, ctx.get(),
                [](gpointer p) { delete static_cast<SniForwardContext*>(p); }, &error);

            if (reg_id == 0) {
                Log::error() << "Failed to register " << iface->name << " at " << path << " for "
                             << item.source_bus_name.c_str() << ": "
                             << (error ? error->message : "Unknown");
                // ctx auto-deleted
                continue;
            }

            ctx.release(); // GLib owns it via destroy notify
            registered_count++;
            g_hash_table_insert(proxied_obj->registration_ids, g_strdup(iface->name),
                                GUINT_TO_POINTER(reg_id));
            g_hash_table_insert(item.registered_objects, GUINT_TO_POINTER(reg_id),
                                g_strdup_printf("%s:%s", path, iface->name));

            Log::info() << "Registered " << iface->name << " at " << path << " for "
                        << item.source_bus_name.c_str() << " (id=" << reg_id << ")";
        }
    }

    if (registered_count > 0) {
        g_hash_table_insert(item.proxied_objects, g_strdup(path), proxied_obj.release());
    }
    // else: proxied_obj auto-deleted

    char* cache_key = g_strdup_printf("%s:%s", item.source_bus_name.c_str(), path);
    g_hash_table_insert(item.node_info_cache, cache_key, g_dbus_node_info_ref(node_info));

    return TRUE;
}

// --- Introspect and register a single path for an SNI item ---

gboolean SniProxy::introspect_and_register_path(SniItem& item, const char* path) {
    if (g_hash_table_contains(item.proxied_objects, path))
        return TRUE;

    Log::info() << "[CALL] Introspect " << path << " -> " << item.source_bus_name.c_str();
    g_autoptr(GError) error = nullptr;
    g_autoptr(GVariant) xml_variant = g_dbus_connection_call_sync(
        source_bus_, item.source_bus_name.c_str(), path, DBUS_INTERFACE_INTROSPECTABLE,
        "Introspect", nullptr, G_VARIANT_TYPE("(s)"), G_DBUS_CALL_FLAGS_NONE, 10000, nullptr,
        &error);

    if (!xml_variant) {
        Log::error() << "[REPLY] Introspect " << path << " on " << item.source_bus_name.c_str()
                     << " FAILED: " << (error ? error->message : "Unknown");
        return FALSE;
    }

    const char* xml_data;
    g_variant_get(xml_variant, "(&s)", &xml_data);
    Log::info() << "[REPLY] Introspect " << path << " on " << item.source_bus_name.c_str()
                << " OK:\n"
                << xml_data;

    g_autoptr(GDBusNodeInfo) node_info = g_dbus_node_info_new_for_xml(xml_data, &error);

    if (!node_info) {
        Log::error() << "Failed to parse XML for " << path << " on " << item.source_bus_name.c_str()
                     << ": " << (error ? error->message : "Unknown");
        return FALSE;
    }

    auto proxied_obj = std::make_unique<SniProxiedObject>(path, node_info);

    guint registered_count = 0;

    if (node_info->interfaces) {
        for (int i = 0; node_info->interfaces[i]; i++) {
            GDBusInterfaceInfo* iface = node_info->interfaces[i];

            if (g_strv_contains(standard_interfaces_, iface->name))
                continue;

            auto ctx = std::make_unique<SniForwardContext>(path, item.source_bus_name, *this);

            guint reg_id = g_dbus_connection_register_object(
                item.target_conn.get(), path, iface, &item_vtable_, ctx.get(),
                [](gpointer p) { delete static_cast<SniForwardContext*>(p); }, &error);

            if (reg_id == 0) {
                Log::error() << "Failed to register " << iface->name << " at " << path << " for "
                             << item.source_bus_name.c_str() << ": "
                             << (error ? error->message : "Unknown");
                // ctx auto-deleted
                continue;
            }

            ctx.release(); // GLib owns it via destroy notify
            registered_count++;
            g_hash_table_insert(proxied_obj->registration_ids, g_strdup(iface->name),
                                GUINT_TO_POINTER(reg_id));
            g_hash_table_insert(item.registered_objects, GUINT_TO_POINTER(reg_id),
                                g_strdup_printf("%s:%s", path, iface->name));

            Log::info() << "Registered " << iface->name << " at " << path << " for "
                        << item.source_bus_name.c_str() << " (id=" << reg_id << ")";
        }
    }

    if (registered_count > 0) {
        g_hash_table_insert(item.proxied_objects, g_strdup(path), proxied_obj.release());
    }
    // else: proxied_obj auto-deleted

    // Recurse into child nodes
    if (node_info->nodes) {
        for (int i = 0; node_info->nodes[i]; i++) {
            const char* child_name = node_info->nodes[i]->path;
            if (!child_name || child_name[0] == '\0')
                continue;

            g_autofree char* child_path = g_str_has_suffix(path, "/")
                                              ? g_strdup_printf("%s%s", path, child_name)
                                              : g_strdup_printf("%s/%s", path, child_name);

            introspect_and_register_path(item, child_path);
        }
    }

    // Cache node_info
    char* cache_key = g_strdup_printf("%s:%s", item.source_bus_name.c_str(), path);
    g_hash_table_insert(item.node_info_cache, cache_key, g_dbus_node_info_ref(node_info));

    return TRUE;
}

// --- Discover dbusmenu path from Menu property ---

void SniProxy::discover_menu_path(SniItem& item) {
    Log::info() << "[CALL] Get " << SNI_ITEM_INTERFACE << ".Menu on " << SNI_ITEM_OBJECT_PATH
                << " -> dest=" << item.source_bus_name.c_str();

    g_autoptr(GError) error = nullptr;
    g_autoptr(GVariant) result = g_dbus_connection_call_sync(
        source_bus_, item.source_bus_name.c_str(), SNI_ITEM_OBJECT_PATH, DBUS_INTERFACE_PROPERTIES,
        "Get", g_variant_new("(ss)", SNI_ITEM_INTERFACE, "Menu"), G_VARIANT_TYPE("(v)"),
        G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, &error);

    if (!result) {
        Log::error() << "[REPLY] Get " << SNI_ITEM_INTERFACE << ".Menu FAILED on "
                     << item.source_bus_name.c_str() << ": "
                     << (error ? error->message : "Unknown");
        return;
    }

    g_autoptr(GVariant) value = nullptr;
    g_variant_get(result, "(v)", &value);
    g_autofree char* value_str = g_variant_print(value, TRUE);
    Log::info() << "[REPLY] Get " << SNI_ITEM_INTERFACE << ".Menu = " << value_str;

    if (g_variant_is_of_type(value, G_VARIANT_TYPE_OBJECT_PATH)) {
        const char* menu_path = g_variant_get_string(value, nullptr);

        if (menu_path && menu_path[0] == '/' && g_strcmp0(menu_path, "/") != 0 &&
            g_strcmp0(menu_path, "/NO_MENU") != 0) {
            Log::info() << "Discovered menu path " << menu_path << " for "
                        << item.source_bus_name.c_str();
            introspect_and_register_path(item, menu_path);
        } else {
            Log::info() << "Menu path '" << (menu_path ? menu_path : "(null)")
                        << "' is not a valid menu, skipping";
        }
    } else {
        Log::info() << "Menu property is not an object path type, skipping";
    }
}

// --- Item name ownership callbacks ---

void SniProxy::on_item_name_acquired(G_GNUC_UNUSED GDBusConnection* conn, const gchar* name,
                                     gpointer user_data) {
    SniProxy& self = *static_cast<SniProxy*>(user_data);
    Log::info() << "Acquired name " << name << " on target bus";

    // Find item by target_bus_name (may differ from source_bus_name)
    SniItem* item = nullptr;
    GHashTableIter iter;
    gpointer key, value;
    g_hash_table_iter_init(&iter, self.sni_items_);
    while (g_hash_table_iter_next(&iter, &key, &value)) {
        SniItem* candidate = static_cast<SniItem*>(value);
        if (candidate->target_bus_name == name) {
            item = candidate;
            break;
        }
    }
    if (!item) {
        Log::error() << "SniItem for target name " << name << " not found";
        return;
    }

    // Register SNI interface using hardcoded XML (no sync introspection).
    // This avoids deadlock: apps that call RegisterStatusNotifierItem
    // synchronously block until our reply is sent. If we do a sync Introspect
    // here, the main loop can't send the reply → both sides block → timeout.
    if (!self.register_path_with_xml(*item, item->item_object_path.c_str(), SNI_ITEM_XML)) {
        Log::error() << "Failed to register SNI interface for " << item->source_bus_name.c_str();
        return;
    }

    // Register dbusmenu at the standard path (used by most SNI apps)
    static const char* DBUSMENU_PATH = "/com/canonical/dbusmenu";
    if (!self.register_path_with_xml(*item, DBUSMENU_PATH, DBUSMENU_ITEM_XML)) {
        Log::info() << "dbusmenu registration at " << DBUSMENU_PATH << " failed for "
                    << item->source_bus_name.c_str() << " (non-fatal)";
    }

    // Subscribe to all signals from this source name on source bus
    item->signal_subscription_id = g_dbus_connection_signal_subscribe(
        self.source_bus_, item->source_bus_name.c_str(), nullptr, nullptr, nullptr, nullptr,
        G_DBUS_SIGNAL_FLAGS_NONE, on_signal_received,
        &self, // user_data = SniProxy*
        nullptr);

    item->ready = TRUE;

    // Add to source watcher's registered items list
    g_ptr_array_add(self.registered_items_, g_strdup(item->source_bus_name.c_str()));

    // Emit StatusNotifierItemRegistered on source bus (for source-side tracking)
    g_autoptr(GError) error = nullptr;
    g_dbus_connection_emit_signal(self.source_bus_, nullptr, SNI_WATCHER_OBJECT_PATH,
                                  SNI_WATCHER_INTERFACE, SNI_SIGNAL_ITEM_REGISTERED,
                                  g_variant_new("(s)", item->source_bus_name.c_str()), &error);
    if (error) {
        Log::error() << "Failed to emit ItemRegistered for " << item->source_bus_name.c_str()
                     << ": " << error->message;
    }

    // Register with the target bus's existing watcher (the desktop panel)
    Log::info() << "[CALL] RegisterStatusNotifierItem('" << name << "') -> " << SNI_WATCHER_BUS_NAME
                << " on target bus";

    auto reg_ctx = std::make_unique<SniMethodCallContext>();
    reg_ctx->invocation = nullptr; // no invocation to reply to
    reg_ctx->forward_bus_name = SNI_WATCHER_BUS_NAME;

    // IMPORTANT: call RegisterStatusNotifierItem from item->target_conn, NOT
    // from target_bus_. The StatusNotifierWatcher tracks which D-Bus connection
    // sent the registration and watches that connection for disappearance.
    // When target_conn closes (on item removal), the watcher automatically
    // detects the caller is gone and emits StatusNotifierItemUnregistered.
    // Using target_bus_ would tie the lifetime to the proxy process itself.
    g_dbus_connection_call(
        item->target_conn.get(), SNI_WATCHER_BUS_NAME, SNI_WATCHER_OBJECT_PATH,
        SNI_WATCHER_INTERFACE, "RegisterStatusNotifierItem", g_variant_new("(s)", name), nullptr,
        G_DBUS_CALL_FLAGS_NONE, 5000, nullptr,
        [](GObject* source, GAsyncResult* res, gpointer user_data) {
            auto ctx = std::unique_ptr<SniMethodCallContext>(
                static_cast<SniMethodCallContext*>(user_data));
            GError* err = nullptr;
            GVariant* result = g_dbus_connection_call_finish(G_DBUS_CONNECTION(source), res, &err);
            if (result) {
                Log::info() << "[REPLY] RegisterStatusNotifierItem to target watcher OK";
                g_variant_unref(result);
            } else {
                Log::error() << "[REPLY] RegisterStatusNotifierItem to target watcher ";
                g_clear_error(&err);
            }
            // ctx unique_ptr destructor frees forward_bus_name
        },
        reg_ctx.release());

    Log::info() << "SNI item " << item->source_bus_name.c_str() << " -> " << name
                << " fully proxied";
}

void SniProxy::on_item_name_lost(G_GNUC_UNUSED GDBusConnection* conn, const gchar* name,
                                 G_GNUC_UNUSED gpointer user_data) {
    Log::error() << "Lost name " << name << " on target bus";
}

// --- Discover and proxy a single SNI item ---

void SniProxy::discover_and_proxy_item(const char* bus_name, const char* object_path) {
    // For unique names (:1.XX), try to resolve to well-known SNI name first.
    // If not found, proxy directly with a generated target name (works when
    // xdg-dbus-proxy runs without --filter).
    if (bus_name[0] == ':') {
        char* well_known = resolve_unique_to_wellknown(bus_name);
        if (well_known) {
            if (!g_hash_table_contains(sni_items_, well_known)) {
                discover_and_proxy_item(well_known, object_path);
            } else {
                Log::verbose() << "SNI item " << well_known << " already tracked";
            }
            g_free(well_known);
            return;
        }
        // No well-known name found — proxy directly with unique name.
        // xdg-dbus-proxy must be running without --filter for this to work.
        Log::info() << "No well-known name for " << bus_name << ", proxying with unique name";
        // Fall through to proxy directly.
    }

    if (g_hash_table_contains(sni_items_, bus_name)) {
        Log::verbose() << "SNI item " << bus_name << " already tracked";
        return;
    }

    Log::info() << "Discovering SNI item: " << bus_name;

    auto item_owner = std::make_unique<SniItem>();
    SniItem* item = item_owner.get();
    item->source_bus_name = bus_name;
    item->item_object_path = object_path ? object_path : SNI_ITEM_OBJECT_PATH;
    item->proxied_objects = g_hash_table_new_full(g_str_hash, g_str_equal, g_free, [](gpointer p) {
        delete static_cast<SniProxiedObject*>(p);
    });
    item->registered_objects =
        g_hash_table_new_full(g_direct_hash, g_direct_equal, nullptr, g_free);
    item->node_info_cache = g_hash_table_new_full(g_str_hash, g_str_equal, g_free,
                                                  (GDestroyNotify)g_dbus_node_info_unref);

    // Target bus name: same as source for well-known names,
    // generated for unique names (fallback when PID resolution fails)
    if (bus_name[0] == ':') {
        g_autofree gchar* gen_name =
            g_strdup_printf("org.kde.StatusNotifierItem-proxy-%d-%d", getpid(), ++proxy_counter_);
        item->target_bus_name = gen_name;
    } else {
        item->target_bus_name = bus_name;
    }

    g_hash_table_insert(sni_items_, g_strdup(bus_name), item_owner.release());

    // Look up and store the unique D-Bus name (:1.X) that owns this item.
    // When the unique name vanishes (app quit), we can clean up even if the
    // well-known name stays alive on the source bus (cross-VM bridge keepalive).
    if (bus_name[0] != ':') {
        g_autoptr(GError) owner_err = nullptr;
        g_autoptr(GVariant) owner_result = g_dbus_connection_call_sync(
            source_bus_, DBUS_INTERFACE_DBUS, DBUS_OBJECT_PATH_DBUS, DBUS_INTERFACE_DBUS,
            "GetNameOwner", g_variant_new("(s)", bus_name), G_VARIANT_TYPE("(s)"),
            G_DBUS_CALL_FLAGS_NONE, 2000, nullptr, &owner_err);
        if (owner_result) {
            const char* unique_name;
            g_variant_get(owner_result, "(&s)", &unique_name);
            item->source_unique_name = unique_name;
            Log::info() << "Stored unique name for " << bus_name << ": " << unique_name;
        } else {
            Log::info() << "Could not get unique name for " << bus_name << ": "
                        << (owner_err ? owner_err->message : "unknown");
        }
    }

    // Create a dedicated connection to the target bus for this item.
    // Each item needs its own connection so that multiple items can each
    // register at /StatusNotifierItem without conflicting — GDBus only allows
    // one object per (path, interface) pair per connection.
    g_autoptr(GError) conn_err = nullptr;
    g_autofree gchar* target_addr =
        g_dbus_address_get_for_bus_sync(target_bus_type_, nullptr, &conn_err);
    if (!target_addr) {
        Log::error() << "Failed to get target bus address for " << bus_name << ": "
                     << (conn_err ? conn_err->message : "Unknown");
        g_hash_table_remove(sni_items_, bus_name);
        return;
    }

    item->target_conn.reset(g_dbus_connection_new_for_address_sync(
        target_addr,
        (GDBusConnectionFlags)(G_DBUS_CONNECTION_FLAGS_AUTHENTICATION_CLIENT |
                               G_DBUS_CONNECTION_FLAGS_MESSAGE_BUS_CONNECTION),
        nullptr, nullptr, &conn_err));

    if (!item->target_conn) {
        Log::error() << "Failed to create target connection for " << bus_name << ": "
                     << (conn_err ? conn_err->message : "Unknown");
        g_hash_table_remove(sni_items_, bus_name);
        return;
    }

    Log::info() << "Created per-item target connection for " << bus_name;

    // Own the target bus name on the per-item connection
    item->target_name_owner_id = g_bus_own_name_on_connection(
        item->target_conn.get(), item->target_bus_name.c_str(), G_BUS_NAME_OWNER_FLAGS_NONE,
        on_item_name_acquired, on_item_name_lost,
        this, // user_data = SniProxy*
        nullptr);

    if (item->target_name_owner_id == 0) {
        Log::error() << "Failed to request name " << item->target_bus_name.c_str()
                     << " on target bus";
        g_hash_table_remove(sni_items_, bus_name);
    }
}

// --- Remove a proxied SNI item ---

void SniProxy::remove_item(const char* bus_name) {
    SniItem* item = static_cast<SniItem*>(g_hash_table_lookup(sni_items_, bus_name));
    if (!item) {
        Log::verbose() << "SNI item " << bus_name << " not tracked, nothing to remove";
        return;
    }

    // Unsubscribe signals before hash table removal
    if (item->signal_subscription_id && source_bus_) {
        g_dbus_connection_signal_unsubscribe(source_bus_, item->signal_subscription_id);
        item->signal_subscription_id = 0;
    }

    // Unregister objects from per-item target connection
    if (item->registered_objects && item->target_conn) {
        GHashTableIter iter;
        gpointer key;
        g_hash_table_iter_init(&iter, item->registered_objects);
        while (g_hash_table_iter_next(&iter, &key, nullptr)) {
            g_dbus_connection_unregister_object(item->target_conn.get(), GPOINTER_TO_UINT(key));
        }
    }

    Log::info() << "Removing target name " << item->target_bus_name.c_str()
                << " (source=" << bus_name << ")";

    // Step 1: explicitly call ReleaseName on the target bus daemon.
    // This is the most direct way to release the name: it tells the daemon
    // to broadcast NameOwnerChanged immediately, letting COSMIC detect the
    // removal without relying on connection teardown timing.
    if (item->target_conn && !item->target_bus_name.empty() &&
        !g_dbus_connection_is_closed(item->target_conn.get())) {
        g_autoptr(GError) rn_err = nullptr;
        g_autoptr(GVariant) rn_result = g_dbus_connection_call_sync(
            item->target_conn.get(), DBUS_INTERFACE_DBUS, DBUS_OBJECT_PATH_DBUS,
            DBUS_INTERFACE_DBUS, "ReleaseName", g_variant_new("(s)", item->target_bus_name.c_str()),
            G_VARIANT_TYPE("(u)"), G_DBUS_CALL_FLAGS_NONE, 2000, nullptr, &rn_err);
        if (rn_result) {
            guint32 ret;
            g_variant_get(rn_result, "(u)", &ret);
            // 1=released, 2=non-existent, 3=not-owner
            Log::info() << "ReleaseName(" << item->target_bus_name.c_str() << ") = " << ret;
        } else {
            Log::error() << "ReleaseName(" << item->target_bus_name.c_str()
                         << ") failed: " << (rn_err ? rn_err->message : "unknown");
        }
    }

    // Step 2: clean up name ownership handle
    if (item->target_name_owner_id) {
        g_bus_unown_name(item->target_name_owner_id);
        item->target_name_owner_id = 0;
    }

    // Step 3: close and release connection
    if (item->target_conn) {
        g_autoptr(GError) close_err = nullptr;
        g_dbus_connection_close_sync(item->target_conn.get(), nullptr, &close_err);
        Log::info() << "target_conn close for " << item->target_bus_name.c_str() << ": "
                    << (close_err ? close_err->message : "ok") << " (is_closed="
                    << (g_dbus_connection_is_closed(item->target_conn.get()) ? "yes" : "no") << ")";
        item->target_conn.reset(); // GConnDeleter skips close (already closed)
    }

    // Remove from registered_items array (uses source_bus_name)
    for (guint i = 0; i < registered_items_->len; i++) {
        if (g_strcmp0(static_cast<const char*>(g_ptr_array_index(registered_items_, i)),
                      bus_name) == 0) {
            g_ptr_array_remove_index(registered_items_, i);
            break;
        }
    }

    // Emit StatusNotifierItemUnregistered on source bus
    g_autoptr(GError) error = nullptr;
    g_dbus_connection_emit_signal(source_bus_, nullptr, SNI_WATCHER_OBJECT_PATH,
                                  SNI_WATCHER_INTERFACE, SNI_SIGNAL_ITEM_UNREGISTERED,
                                  g_variant_new("(s)", bus_name), &error);
    if (error) {
        Log::error() << "Failed to emit ItemUnregistered for " << bus_name << ": "
                     << error->message;
    }

    // Remove from hash table (triggers ~SniItem() via delete destroy callback)
    g_hash_table_remove(sni_items_, bus_name);
    Log::info() << "SNI item " << bus_name << " removed";
}

// --- Source bus monitoring: NameOwnerChanged ---

void SniProxy::on_name_owner_changed(G_GNUC_UNUSED GDBusConnection* connection,
                                     G_GNUC_UNUSED const char* sender_name,
                                     G_GNUC_UNUSED const char* object_path,
                                     G_GNUC_UNUSED const char* interface_name,
                                     G_GNUC_UNUSED const char* signal_name, GVariant* parameters,
                                     gpointer user_data) {
    SniProxy& self = *static_cast<SniProxy*>(user_data);

    const char *name, *old_owner, *new_owner;
    g_variant_get(parameters, "(&s&s&s)", &name, &old_owner, &new_owner);

    // Log ALL NameOwnerChanged signals for debugging
    Log::verbose() << "[SIGNAL] NameOwnerChanged: name=" << " old_owner='"
                   << "' ";

    if (!g_str_has_prefix(name, SNI_ITEM_BUS_NAME_PREFIX)) {
        if (name[0] == ':' && old_owner[0] != '\0' && new_owner[0] == '\0') {
            // Unique name vanished: clean up pending registrations
            if (g_hash_table_contains(self.pending_items_, name)) {
                Log::info() << "Pending item " << name
                            << " disconnected before resolution, removing";
                g_hash_table_remove(self.pending_items_, name);
            }

            // Also check if any proxied item was owned by this unique name.
            // This handles cross-VM scenarios where the well-known SNI name stays
            // alive on the source bus (bridge keepalive) even after the app quits.
            // We stored source_unique_name in discover_and_proxy_item() for this.
            GPtrArray* to_remove = g_ptr_array_new_with_free_func(g_free);
            GHashTableIter uiter;
            gpointer ukey, uval;
            g_hash_table_iter_init(&uiter, self.sni_items_);
            while (g_hash_table_iter_next(&uiter, &ukey, &uval)) {
                SniItem* candidate = static_cast<SniItem*>(uval);
                if (candidate->source_unique_name == name) {
                    g_ptr_array_add(to_remove, g_strdup(static_cast<const char*>(ukey)));
                }
            }
            for (guint i = 0; i < to_remove->len; i++) {
                const char* sname = static_cast<const char*>(g_ptr_array_index(to_remove, i));
                Log::info() << "App unique name " << name << " vanished → removing stale item "
                            << sname;
                self.remove_item(sname);
            }
            g_ptr_array_free(to_remove, TRUE);

            // Also handle items tracked directly by unique name (no well-known name)
            if (g_hash_table_contains(self.sni_items_, name)) {
                Log::info() << "Directly proxied unique name " << name << " vanished";
                self.remove_item(name);
            }
        }
        return;
    }

    gboolean appeared = (new_owner[0] != '\0' && old_owner[0] == '\0');
    gboolean vanished = (old_owner[0] != '\0' && new_owner[0] == '\0');

    if (appeared) {
        Log::info() << "SNI item appeared: " << name << " (owner: " << new_owner << ")";

        // Check if this resolves a pending unique-name registration.
        // When an app registers with unique name (:1.XX) but we can't resolve it
        // immediately, we defer. Now NameOwnerChanged brings the well-known name.
        if (g_hash_table_contains(self.pending_items_, new_owner)) {
            Log::info() << "Resolved pending item " << new_owner << " -> " << name;
            g_hash_table_remove(self.pending_items_, new_owner);
        }

        self.discover_and_proxy_item(name);
    } else if (vanished) {
        Log::info() << "SNI item vanished: " << name << " (was: " << old_owner << ")";
        self.remove_item(name);
    }
}

gboolean SniProxy::subscribe_name_owner_changed() {
    name_owner_changed_sub_ = g_dbus_connection_signal_subscribe(
        source_bus_, DBUS_INTERFACE_DBUS, DBUS_INTERFACE_DBUS, DBUS_SIGNAL_NAME_OWNER_CHANGED,
        DBUS_OBJECT_PATH_DBUS, nullptr, G_DBUS_SIGNAL_FLAGS_NONE, on_name_owner_changed,
        this, // user_data
        nullptr);

    return (name_owner_changed_sub_ != 0);
}

// --- Initial scan for existing SNI items ---

void SniProxy::initial_scan() {
    g_autoptr(GError) error = nullptr;
    g_autoptr(GVariant) result = g_dbus_connection_call_sync(
        source_bus_, DBUS_INTERFACE_DBUS, DBUS_OBJECT_PATH_DBUS, DBUS_INTERFACE_DBUS, "ListNames",
        nullptr, G_VARIANT_TYPE("(as)"), G_DBUS_CALL_FLAGS_NONE, 5000, nullptr, &error);

    if (!result) {
        Log::error() << "ListNames failed: " << (error ? error->message : "Unknown");
        return;
    }

    GVariantIter* iter;
    const char* name;
    g_variant_get(result, "(as)", &iter);
    while (g_variant_iter_next(iter, "&s", &name)) {
        if (g_str_has_prefix(name, SNI_ITEM_BUS_NAME_PREFIX)) {
            Log::info() << "Found existing SNI item: " << name;
            if (!g_hash_table_contains(sni_items_, name)) {
                discover_and_proxy_item(name);
            }
        }
    }
    g_variant_iter_free(iter);
}
