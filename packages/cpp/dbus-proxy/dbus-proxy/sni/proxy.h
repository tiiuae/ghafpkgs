/*
 Copyright 2025 TII (SSRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#pragma once

#include "gdbusprivate.h"
#include "glib_raii.h"
#include <gio/gio.h>
#include <glib.h>
#include <string>

// Forward declaration to avoid pulling in full GDK headers here
typedef struct _GdkDisplay GdkDisplay;

// Tracks a single proxied SNI item (one tray app)
struct SniItem {
    std::string source_bus_name;    // e.g. ":1.234" or "org.kde.StatusNotifierItem-1234-1"
    std::string source_unique_name; // unique name (:1.X) of the app that owns this item
    std::string target_bus_name;    // name owned on target bus (may be generated proxy name)
    std::string item_object_path;   // object path (e.g. "/StatusNotifierItem")

    GConnPtr target_conn;                     // per-item connection to target bus;
                                              // close()+unref() handled by GConnPtr destructor
    guint target_name_owner_id = 0;           // g_bus_own_name() return for target bus
    GHashTable* proxied_objects = nullptr;    // object_path -> ProxiedObject*
    GHashTable* registered_objects = nullptr; // reg_id -> string (for cleanup)
    GHashTable* node_info_cache = nullptr;    // cache key -> GDBusNodeInfo*
    guint signal_subscription_id = 0;         // catch-all signal sub for this source
    gboolean ready = FALSE;                   // TRUE once introspection + registration done

    ~SniItem(); // defined in sni-proxy.cpp
};

// Context for forwarding calls to a specific source bus name
struct SniForwardContext {
    std::string object_path;
    std::string source_bus_name;
    class SniProxy& proxy; // back-reference to owning SniProxy (outlives this struct)

    SniForwardContext(std::string op, std::string sbn, class SniProxy& p)
        : object_path(std::move(op)), source_bus_name(std::move(sbn)), proxy(p) {}
};

// SNI proxy class: manages StatusNotifierItem proxying between two buses
class SniProxy {
  public:
    SniProxy(GDBusConnection* source_bus, GDBusConnection* target_bus, GBusType target_bus_type);
    ~SniProxy();

    // Construct and initialize in one step; returns nullptr on failure.
    static std::unique_ptr<SniProxy> create(GDBusConnection* source_bus,
                                            GDBusConnection* target_bus, GBusType target_bus_type);

    // Used by on_activate_before_event (file-level callback)
    static void method_reply_callback(GObject* source, GAsyncResult* res, gpointer user_data);

  private:
    // Bus connections (not owned by this class)
    GDBusConnection* source_bus_;
    GDBusConnection* target_bus_;
    GBusType target_bus_type_;

    // Watcher state (source bus)
    guint watcher_reg_id_ = 0;
    guint watcher_name_owner_id_ = 0;
    GNodeInfoPtr watcher_node_info_; // owns the parsed watcher introspection XML
    gboolean host_registered_ = FALSE;

    // Item tracking
    GHashTable* sni_items_ = nullptr;       // source_bus_name -> SniItem*
    GPtrArray* registered_items_ = nullptr; // item names for Watcher property
    GHashTable* pending_items_ = nullptr;   // unique_name -> object_path (deferred)

    // Source bus monitoring
    guint name_owner_changed_sub_ = 0;
    int proxy_counter_ = 0; // counter for generated proxy bus names

    // Set to true by constructor on successful initialization
    bool initialized_ = false;

    // GDK Wayland display for XDG activation token generation
    GdkDisplay* wayland_display_ = nullptr;
    gboolean wayland_init_tried_ = FALSE; // avoid repeated scan attempts

    // --- Watcher implementation ---
    gboolean register_watcher();
    gboolean own_watcher_name();
    // --- Item lifecycle ---
    void discover_and_proxy_item(const char* bus_name, const char* object_path = nullptr);
    void remove_item(const char* bus_name);
    void initial_scan();
    gboolean register_path_with_xml(SniItem& item, const char* path, const char* xml);
    gboolean introspect_and_register_path(SniItem& item, const char* path);
    void discover_menu_path(SniItem& item);

    // --- Source bus monitoring ---
    gboolean subscribe_name_owner_changed();

    // --- Wayland lazy init (for XDG activation when service lacks env) ---
    void try_lazy_wayland_init();

    // --- Name resolution ---
    char* resolve_unique_to_wellknown(const char* unique_name);

    // --- Static GDBus callbacks: Watcher (user_data = SniProxy*) ---
    static void on_watcher_method_call(GDBusConnection* connection, const char* sender,
                                       const char* object_path, const char* interface_name,
                                       const char* method_name, GVariant* parameters,
                                       GDBusMethodInvocation* invocation, gpointer user_data);
    static GVariant* on_watcher_get_property(GDBusConnection* connection, const char* sender,
                                             const char* object_path, const char* interface_name,
                                             const char* property_name, GError** error,
                                             gpointer user_data);
    static void on_watcher_name_acquired(GDBusConnection* conn, const gchar* name,
                                         gpointer user_data);
    static void on_watcher_name_lost(GDBusConnection* conn, const gchar* name, gpointer user_data);
    // --- Static GDBus callbacks: per-item (user_data = SniForwardContext*) ---
    static void on_method_call(GDBusConnection* connection, const char* sender,
                               const char* object_path, const char* interface_name,
                               const char* method_name, GVariant* parameters,
                               GDBusMethodInvocation* invocation, gpointer user_data);
    static GVariant* on_get_property(GDBusConnection* connection, const char* sender,
                                     const char* object_path, const char* interface_name,
                                     const char* property_name, GError** error, gpointer user_data);
    static gboolean on_set_property(GDBusConnection* connection, const char* sender,
                                    const char* object_path, const char* interface_name,
                                    const char* property_name, GVariant* value, GError** error,
                                    gpointer user_data);

    // --- Static GDBus callbacks: per-item name ownership ---
    static void on_item_name_acquired(GDBusConnection* conn, const gchar* name, gpointer user_data);
    static void on_item_name_lost(GDBusConnection* conn, const gchar* name, gpointer user_data);

    // --- Static GDBus callbacks: signal forwarding & monitoring ---
    static void on_signal_received(GDBusConnection* connection, const char* sender_name,
                                   const char* object_path, const char* interface_name,
                                   const char* signal_name, GVariant* parameters,
                                   gpointer user_data);
    static void on_name_owner_changed(GDBusConnection* connection, const char* sender_name,
                                      const char* object_path, const char* interface_name,
                                      const char* signal_name, GVariant* parameters,
                                      gpointer user_data);

    // Watcher introspection XML
    static constexpr const char* WATCHER_XML =
        "<node>"
        "  <interface name='org.kde.StatusNotifierWatcher'>"
        "    <method name='RegisterStatusNotifierItem'>"
        "      <arg name='service' type='s' direction='in'/>"
        "    </method>"
        "    <method name='RegisterStatusNotifierHost'>"
        "      <arg name='service' type='s' direction='in'/>"
        "    </method>"
        "    <property name='RegisteredStatusNotifierItems' type='as' "
        "access='read'/>"
        "    <property name='IsStatusNotifierHostRegistered' type='b' "
        "access='read'/>"
        "    <property name='ProtocolVersion' type='i' access='read'/>"
        "    <signal name='StatusNotifierItemRegistered'>"
        "      <arg name='service' type='s'/>"
        "    </signal>"
        "    <signal name='StatusNotifierItemUnregistered'>"
        "      <arg name='service' type='s'/>"
        "    </signal>"
        "    <signal name='StatusNotifierHostRegistered'/>"
        "  </interface>"
        "</node>";

    // Hardcoded interface XML (avoids runtime introspection deadlock)
    static constexpr const char* SNI_ITEM_XML =
        "<node>"
        "  <interface name='org.kde.StatusNotifierItem'>"
        "    <property name='Category' type='s' access='read'/>"
        "    <property name='Id' type='s' access='read'/>"
        "    <property name='Title' type='s' access='read'/>"
        "    <property name='Status' type='s' access='read'/>"
        "    <property name='WindowId' type='i' access='read'/>"
        "    <property name='IconName' type='s' access='read'/>"
        "    <property name='IconPixmap' type='a(iiay)' access='read'/>"
        "    <property name='OverlayIconName' type='s' access='read'/>"
        "    <property name='OverlayIconPixmap' type='a(iiay)' access='read'/>"
        "    <property name='AttentionIconName' type='s' access='read'/>"
        "    <property name='AttentionIconPixmap' type='a(iiay)' access='read'/>"
        "    <property name='AttentionMovieName' type='s' access='read'/>"
        "    <property name='ToolTip' type='(sa(iiay)ss)' access='read'/>"
        "    <property name='ItemIsMenu' type='b' access='read'/>"
        "    <property name='Menu' type='o' access='read'/>"
        "    <property name='IconThemePath' type='s' access='read'/>"
        "    <method name='ContextMenu'>"
        "      <arg name='x' type='i' direction='in'/>"
        "      <arg name='y' type='i' direction='in'/>"
        "    </method>"
        "    <method name='Activate'>"
        "      <arg name='x' type='i' direction='in'/>"
        "      <arg name='y' type='i' direction='in'/>"
        "    </method>"
        "    <method name='SecondaryActivate'>"
        "      <arg name='x' type='i' direction='in'/>"
        "      <arg name='y' type='i' direction='in'/>"
        "    </method>"
        "    <method name='Scroll'>"
        "      <arg name='delta' type='i' direction='in'/>"
        "      <arg name='orientation' type='s' direction='in'/>"
        "    </method>"
        "    <method name='ProvideXdgActivationToken'>"
        "      <arg name='token' type='s' direction='in'/>"
        "    </method>"
        "    <signal name='NewTitle'/>"
        "    <signal name='NewIcon'/>"
        "    <signal name='NewAttentionIcon'/>"
        "    <signal name='NewOverlayIcon'/>"
        "    <signal name='NewToolTip'/>"
        "    <signal name='NewStatus'>"
        "      <arg name='status' type='s'/>"
        "    </signal>"
        "    <signal name='NewIconThemePath'>"
        "      <arg name='icon_theme_path' type='s'/>"
        "    </signal>"
        "  </interface>"
        "</node>";

    static constexpr const char* DBUSMENU_ITEM_XML =
        "<node>"
        "  <interface name='com.canonical.dbusmenu'>"
        "    <property name='Version' type='u' access='read'/>"
        "    <property name='TextDirection' type='s' access='read'/>"
        "    <property name='Status' type='s' access='read'/>"
        "    <property name='IconThemePath' type='as' access='read'/>"
        "    <method name='GetLayout'>"
        "      <arg name='parentId' type='i' direction='in'/>"
        "      <arg name='recursionDepth' type='i' direction='in'/>"
        "      <arg name='propertyNames' type='as' direction='in'/>"
        "      <arg name='revision' type='u' direction='out'/>"
        "      <arg name='layout' type='(ia{sv}av)' direction='out'/>"
        "    </method>"
        "    <method name='GetGroupProperties'>"
        "      <arg name='ids' type='ai' direction='in'/>"
        "      <arg name='propertyNames' type='as' direction='in'/>"
        "      <arg name='properties' type='a(ia{sv})' direction='out'/>"
        "    </method>"
        "    <method name='GetProperty'>"
        "      <arg name='id' type='i' direction='in'/>"
        "      <arg name='name' type='s' direction='in'/>"
        "      <arg name='value' type='v' direction='out'/>"
        "    </method>"
        "    <method name='Event'>"
        "      <arg name='id' type='i' direction='in'/>"
        "      <arg name='eventId' type='s' direction='in'/>"
        "      <arg name='data' type='v' direction='in'/>"
        "      <arg name='timestamp' type='u' direction='in'/>"
        "    </method>"
        "    <method name='EventGroup'>"
        "      <arg name='events' type='a(isvu)' direction='in'/>"
        "      <arg name='idErrors' type='ai' direction='out'/>"
        "    </method>"
        "    <method name='AboutToShow'>"
        "      <arg name='id' type='i' direction='in'/>"
        "      <arg name='needUpdate' type='b' direction='out'/>"
        "    </method>"
        "    <method name='AboutToShowGroup'>"
        "      <arg name='ids' type='ai' direction='in'/>"
        "      <arg name='updatesNeeded' type='ai' direction='out'/>"
        "      <arg name='idErrors' type='ai' direction='out'/>"
        "    </method>"
        "    <signal name='ItemsPropertiesUpdated'>"
        "      <arg name='updatedProps' type='a(ia{sv})'/>"
        "      <arg name='removedProps' type='a(ias)'/>"
        "    </signal>"
        "    <signal name='LayoutUpdated'>"
        "      <arg name='revision' type='u'/>"
        "      <arg name='parent' type='i'/>"
        "    </signal>"
        "    <signal name='ItemActivationRequested'>"
        "      <arg name='id' type='i'/>"
        "      <arg name='timestamp' type='u'/>"
        "    </signal>"
        "  </interface>"
        "</node>";

    // Vtables for D-Bus interface registration
    static const GDBusInterfaceVTable item_vtable_;
    static const GDBusInterfaceVTable watcher_vtable_;

    // Standard D-Bus interfaces to skip when registering
    static constexpr const char* standard_interfaces_[] = {
        DBUS_INTERFACE_INTROSPECTABLE, DBUS_INTERFACE_PEER, DBUS_INTERFACE_PROPERTIES, nullptr};
};
