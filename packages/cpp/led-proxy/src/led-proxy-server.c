/*
 Copyright 2026 TII (CRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#include <errno.h>
#include <fcntl.h>
#include <getopt.h>
#include <glob.h>
#include <limits.h>
#include <linux/vm_sockets.h>
#include <poll.h>
#include <pthread.h>
#include <semaphore.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/param.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <unistd.h>

#include "common/common.h"
#include "common/log.h"
#include "common/protocol.h"

#define LEDS_SYSFS_BASE "/sys/class/leds/"
#define MAX_CONNECTIONS 10
#define LED_NAME_LEN_MAX 64

static int led_count = 0;
static int connections_count = 0;
static int max_connections = MAX_CONNECTIONS;
static sem_t connections_sem;
static unsigned int client_cid[MAX_CONNECTIONS];
static struct leds {
  char name[LED_NAME_LEN_MAX];
  enum led_role {
    LED_ROLE_UNKNOWN = 0,
    LED_ROLE_NORMAL,
    LED_ROLE_MONITORED,
  } role;
} leds[MAX_LEDS];

struct client_data {
  unsigned int cid;
  unsigned int fd;
  int client_index;
};

static int resolve_led_pattern(const char *pattern, enum led_role role) {
  char path[PATH_MAX];
  glob_t result = {0};

  if (pattern == NULL || pattern[0] == '\0') {
    LOG_ERROR("invalid empty LED pattern");
    return -1;
  }
  int len = snprintf(path, sizeof(path), LEDS_SYSFS_BASE "/%s", pattern);
  if (len < 0 || (size_t)len >= sizeof(path)) {
    LOG_ERROR("LED pattern is too long: '%s'", pattern);
    return -1;
  }

  int ret = glob(path, GLOB_ERR, NULL, &result);

  if (ret == GLOB_NOMATCH) {
    LOG_ERROR("no LED matches pattern '%s'", pattern);
    globfree(&result);
    return -1;
  }

  if (ret != 0) {
    LOG_ERROR("glob failed for LED pattern '%s': %d", pattern, ret);
    globfree(&result);
    return -1;
  }

  for (size_t i = 0; i < result.gl_pathc; i++) {
    const char *full_path = result.gl_pathv[i];
    const char *name = strrchr(full_path, '/');
    if (name == NULL || name[1] == '\0')
      continue;

    if (led_count >= MAX_LEDS) {
      LOG_ERROR("maximum number of LEDs reached");
      break;
    }
    name++;
    LOG_DEBUG("LED '%s' matched '%s'", pattern, name);

    int exists = 0, j;
    for (j = 0; j < led_count; j++) {
      if (strcmp(leds[j].name, name) == 0) {
        exists = 1;
        break;
      }
    }
    // Determine if this LED can be added or updated based on its role and
    // whether it already exists.
    int allowed = exists && (role == LED_ROLE_NORMAL) &&
                  (leds[j].role == LED_ROLE_MONITORED);
    if (!exists) {
      strncpy(leds[led_count].name, name, LED_NAME_LEN_MAX - 1);
      leds[led_count].name[LED_NAME_LEN_MAX - 1] = '\0';
      leds[led_count].role = role;
      led_count++;
    } else if (allowed) {
      leds[j].role = LED_ROLE_MONITORED;
    }
  }

  globfree(&result);
  return 0;
}

static int host_leds_check_exist(char **arg_led_names, int arg_led_count,
                                 enum led_role role) {

  for (int i = 0; i < arg_led_count; i++) {
    if (strlen(arg_led_names[i]) >= LED_NAME_LEN_MAX) {
      LOG_ERROR("LED name too long: %s", arg_led_names[i]);
      continue;
    }
    if (resolve_led_pattern(arg_led_names[i], role)) {
      continue;
    }
  }
  return led_count;
}

uint16_t get_brightness(const char *led_name, const char *type) {

  char path[PATH_MAX];

  snprintf(path, sizeof(path), LEDS_SYSFS_BASE "%s/%s", led_name, type);
  FILE *f = fopen(path, "r");
  if (!f) {
    LOG_ERROR("fopen: %s", strerror(errno));
    return UINT16_MAX;
  }
  int brightness;
  if (fscanf(f, "%d", &brightness) != 1) {
    LOG_ERROR("fscanf: %s", strerror(errno));
    fclose(f);
    return UINT16_MAX;
  }
  if (brightness < 0 || brightness > UINT16_MAX) {
    LOG_ERROR("Invalid brightness value: %d", brightness);
    fclose(f);
    return UINT16_MAX;
  }
  fclose(f);
  return (uint16_t)brightness;
}

static int host_leds_send_list(const int socket_fd) {

  for (int i = 0; i < led_count; i++) {
    // Send each LED name over the socket here.
    LOG_DEBUG("Sending LED name: %s", leds[i].name);
    struct led_msg_hdr hdr;
    hdr.version = LED_PROXY_VERSION;
    hdr.type = LED_MSG_ADD_LED;
    hdr.length = strlen(leds[i].name) + 1; // Include null terminator
    hdr.max_brightness =
        MIN(get_brightness(leds[i].name, "max_brightness"), 255);
    if (write_all(socket_fd, &hdr, sizeof(hdr)) < 0) {
      LOG_ERROR("write: %s", strerror(errno));
      return -1;
    }
    if (write_all(socket_fd, leds[i].name, hdr.length) < 0) {
      LOG_ERROR("write: %s", strerror(errno));
      return -1;
    }
  }
  return 0;
}

static int host_leds_set(const struct led_msg_hdr *hdr) {

  char buf[PATH_MAX];

  snprintf(buf, sizeof(buf), "%s", LEDS_SYSFS_BASE);
  size_t prefix_len = strlen(LEDS_SYSFS_BASE);

  LOG_DEBUG("Handling LED_MSG_SET_BRIGHTNESS message");

  sprintf(buf + prefix_len, "%s/brightness", leds[hdr->led_index].name);

  FILE *f = fopen(buf, "w");
  if (!f) {
    LOG_ERROR("fopen: %s", strerror(errno));
    return -1;
  }
  fprintf(f, "%d\n", hdr->brightness);
  fclose(f);

  return 0;
}

static void *host_leds_handle_client(const void *arg) {

  struct led_msg_hdr hdr;
  ssize_t bytes_read;
  struct pollfd
      pollfds[MAX_LEDS + 1]; // +1 for the client socket fd, at index 0
  int fds = 1;               // Start with 1 for the client socket fd at index 0
  int monitored_leds[MAX_LEDS];

  memset(pollfds, 0, sizeof(pollfds));

  // Loop over monitored LEDs and open /sys path to brightness_hw_changed
  for (int i = 0; i < led_count; i++) {
    if (leds[i].role != LED_ROLE_MONITORED) {
      continue;
    }

    char buf[PATH_MAX];
    snprintf(buf, sizeof(buf), "%s%s/brightness_hw_changed", LEDS_SYSFS_BASE,
             leds[i].name);
    int fd = open(buf, O_RDONLY);
    if (fd < 0) {
      LOG_ERROR("open: %s: %s", buf, strerror(errno));
      return NULL;
    }
    pollfds[fds].fd = fd;
    pollfds[fds].events = POLLPRI;
    pollfds[fds].revents = 0;
    monitored_leds[fds - 1] = i; // store the monitored LED index

    char dummy[32];
    ssize_t ret = read(fd, dummy, sizeof(dummy));
    (void)ret;
    fds++;
  }

  int client_fd = ((struct client_data *)arg)->fd;
  pollfds[0].fd = client_fd;
  pollfds[0].events = POLLIN;
  pollfds[0].revents = 0;
  for (;;) {
    int ret = poll(pollfds, fds, -1);
    if (ret < 0) {
      LOG_ERROR("poll: %s", strerror(errno));
      return NULL;
    }

    // Check if the client socket has data to read
    if (pollfds[0].revents & POLLIN) {
      bytes_read = read_all(client_fd, &hdr, sizeof(hdr));

      if (bytes_read <= 0) {
        if (bytes_read < 0) {
          LOG_ERROR("read: %s", strerror(errno));
        }
        LOG_DEBUG("Connection closed by client");
        goto exit;
      }
      // check protocol version
      if (hdr.version != LED_PROXY_VERSION) {
        LOG_ERROR("Unsupported protocol version: %u", hdr.version);
        goto exit;
      }

      switch (hdr.type) {
      case LED_MSG_HELLO:
        if (host_leds_send_list(client_fd) < 0) {
          LOG_ERROR("host_leds_send_list failed");
          goto exit;
        }
        break;

      case LED_MSG_SET_BRIGHTNESS:
        // Handle LED_MSG_SET_BRIGHTNESS message here.
        if (hdr.led_index >= led_count) {
          LOG_ERROR("Invalid LED index: %u", hdr.led_index);
          break;
        }
        LOG_DEBUG(
            "Received LED_MSG_SET_BRIGHTNESS message for LED index %u [%s] "
            "with brightness %d",
            hdr.led_index, leds[hdr.led_index].name, hdr.brightness);
        if (host_leds_set(&hdr) < 0) {
          LOG_ERROR("host_leds_set failed");
          goto exit;
        }
        break;

      case LED_MSG_EXIT:
        LOG_DEBUG("Received LED_MSG_EXIT message");
        goto exit;

      default:
        LOG_ERROR("Unknown message type: %u", hdr.type);
        break;
      }
    }
    // Check if any of the LED fds has events
    // i starts from 1 as the zero element is clients fd
    for (int i = 1; i < fds; i++) {
      if (pollfds[i].revents & POLLPRI) {
        // execute a dummy read to clear the event
        char dummy[32];
        ret = read(pollfds[i].fd, &dummy, sizeof(dummy));
        (void)ret;
        (void)lseek(pollfds[i].fd, 0, SEEK_SET);

        int led_index = monitored_leds[i - 1];
        u_int16_t brightness =
            get_brightness(leds[led_index].name, "brightness");
        if (brightness == UINT16_MAX) {
          LOG_ERROR("Failed to get brightness for LED: %s",
                    leds[led_index].name);
          continue;
        }

        LOG_DEBUG("LED event for monitored LED: %s brightness %u",
                  leds[led_index].name, brightness);

        // Notify the client about the brightness change for this LED.
        hdr.type = LED_MSG_BRIGHTNESS_CHANGED;
        hdr.led_index = led_index;
        hdr.brightness = brightness;
        if (write_all(client_fd, &hdr, sizeof(hdr)) < 0) {
          LOG_ERROR("write failed");
          continue;
        }
      }
    }
  }

exit:
  // close all fds associated with this client and leds
  for (int i = 0; i < fds; i++) {
    close(pollfds[i].fd);
  }
  sem_wait(&connections_sem);
  if (((struct client_data *)arg)->client_index != -1) {
    client_cid[((struct client_data *)arg)->client_index] = 0;
  }
  connections_count--;
  sem_post(&connections_sem);
  free((struct client_data *)arg);
  return NULL;
}

static int host_leds_run(unsigned int vsock_port, unsigned int allowed_cid) {

  int listen_fd = socket(AF_VSOCK, SOCK_STREAM, 0);
  if (listen_fd < 0) {
    LOG_ERROR("socket: %s", strerror(errno));
    return -1;
  }

  struct sockaddr_vm addr;
  memset(&addr, 0, sizeof(addr));
  addr.svm_family = AF_VSOCK;
  addr.svm_cid = VMADDR_CID_ANY;
  addr.svm_port = vsock_port;

  if (bind(listen_fd, (struct sockaddr *)&addr, sizeof(addr)) < 0) {
    LOG_ERROR("bind: %s", strerror(errno));
    close(listen_fd);
    return -1;
  }

  if (listen(listen_fd, SOMAXCONN) < 0) {
    LOG_ERROR("listen: %s", strerror(errno));
    close(listen_fd);
    return -1;
  }

  LOG_DEBUG("Listening for guest connections on vsock port %u", vsock_port);

  for (;;) {
    struct sockaddr_vm peer_addr;
    int client_index = -1;
    socklen_t peer_len = sizeof(peer_addr);
    int client_fd = accept(listen_fd, (struct sockaddr *)&peer_addr, &peer_len);
    if (client_fd < 0) {
      LOG_ERROR("accept: %s", strerror(errno));
      break;
    }

    sem_wait(&connections_sem);
    if (connections_count >= max_connections) {
      LOG_ERROR("Maximum number of clients reached (%d)", max_connections);
      goto close_accept_next;
    }

    for (int i = 0; i < max_connections; i++) {
      if (client_cid[i] == peer_addr.svm_cid) {
        LOG_ERROR("Connection from cid %u already exists", peer_addr.svm_cid);
        goto close_accept_next;
      }
    }

    if (allowed_cid != VMADDR_CID_ANY && peer_addr.svm_cid != allowed_cid) {
      LOG_ERROR("Rejecting connection from disallowed cid %u",
                peer_addr.svm_cid);
      goto close_accept_next;
    }

    LOG_DEBUG("Accepted connection from cid %u", peer_addr.svm_cid);
    int i;
    for (i = 0; i < max_connections; i++) {
      if (client_cid[i] == 0) {
        client_cid[i] = peer_addr.svm_cid;
        client_index = i;
        break;
      }
    }
    if (i >= max_connections) {
      LOG_ERROR("No available slot for new client");
      goto close_accept_next;
    }
    connections_count++;
    sem_post(&connections_sem);

    pthread_t thread;
    struct client_data *data = malloc(sizeof(struct client_data));
    if (!data) {
      LOG_ERROR("malloc: %s", strerror(errno));
      goto clear_cid_accept_next;
    }
    data->cid = peer_addr.svm_cid;
    data->fd = client_fd;
    data->client_index = client_index;
    if (pthread_create(&thread, NULL,
                       (void *(*)(void *))host_leds_handle_client,
                       (void *)data) != 0) {
      LOG_ERROR("pthread_create failed");
      free(data);
      goto clear_cid_accept_next;
    }
    pthread_detach(thread);
    continue;

  clear_cid_accept_next:
    sem_wait(&connections_sem);
    connections_count--;
    if (client_index != -1) {
      client_cid[client_index] = 0;
    };

  close_accept_next:
    close(client_fd);
    sem_post(&connections_sem);
  }

  close(listen_fd);

  return 0;
}

static void print_usage(const char *prog_name) {

  fprintf(stderr,
          "Usage: %s [--port PORT] [--cid CID] LED_NAMES...\n"
          "\n"
          "Options:\n"
          "  -n, --clients COUNT  Maximum number of clients to accept "
          "(default: %d)\n"
          "  -p, --port PORT     vsock port to listen on "
          "(default: %d)\n"
          "  -c, --cid CID       Only accept connections from this guest "
          "cid (default: any)\n"
          "  -h, --help          Show this help message\n"
          "\n"
          "Example:\n"
          "  %s --port %d \\\n"
          "      tpacpi::kbd_backlight \\\n"
          "      platform::mute \\\n"
          "      platform::micmute\n",
          prog_name, MAX_CONNECTIONS, LED_PROXY_VSOCK_PORT, prog_name,
          LED_PROXY_VSOCK_PORT);
}

int main(int argc, char *argv[]) {

  unsigned int vsock_port = LED_PROXY_VSOCK_PORT;
  unsigned int allowed_cid = VMADDR_CID_ANY;

  static const struct option long_options[] = {
      {"port", required_argument, NULL, 'p'},
      {"cid", required_argument, NULL, 'c'},
      {"clients", required_argument, NULL, 'n'},
      {"help", no_argument, NULL, 'h'},
      {"monitored-led", required_argument, NULL, 'm'},
      {NULL, 0, NULL, 0}};

  int opt;
  char *tmp_str = NULL;
  memset(leds, 0, sizeof(leds));
  led_count = 0;

  while ((opt = getopt_long(argc, argv, "p:c:n:h:m:", long_options, NULL)) !=
         -1) {
    switch (opt) {
    case 'p':
      ulong tmp_port;
      tmp_str = NULL;
      tmp_port = strtoul(optarg, &tmp_str, 10);
      if ((tmp_port == ULONG_MAX && errno == ERANGE) || tmp_str == optarg ||
          *tmp_str != '\0' || !tmp_port || tmp_port > UINT16_MAX) {
        LOG_ERROR("Invalid port: %s", optarg);
        return 1;
      }
      vsock_port = (unsigned int)tmp_port;
      break;
    case 'c':
      ulong tmp_cid;
      tmp_str = NULL;
      tmp_cid = strtoul(optarg, &tmp_str, 10);
      if ((tmp_cid == ULONG_MAX && errno == ERANGE) || tmp_str == optarg ||
          *tmp_str != '\0' || !tmp_cid || tmp_cid > UINT16_MAX) {
        LOG_ERROR("Invalid CID: %s", optarg);
        return 1;
      }
      allowed_cid = (unsigned int)tmp_cid;
      break;
    case 'n':
      ulong tmp_max;
      tmp_str = NULL;
      tmp_max = strtoul(optarg, &tmp_str, 10);
      if ((tmp_max == ULONG_MAX && errno == ERANGE) || tmp_str == optarg ||
          *tmp_str != '\0' || tmp_max == 0 || tmp_max > MAX_CONNECTIONS) {
        LOG_ERROR("Invalid max connections: %s", optarg);
        return 1;
      }
      max_connections = (unsigned int)tmp_max;
      break;
    case 'h':
      print_usage(argv[0]);
      return 0;
    case 'm':
      if (host_leds_check_exist(&optarg, 1, LED_ROLE_MONITORED) == 0) {
        LOG_ERROR("Monitored LED does not exist under %s", LEDS_SYSFS_BASE);
        return 1;
      }
      break;
    default:
      print_usage(argv[0]);
      return 1;
    }
  }

  memset(client_cid, 0, sizeof(client_cid));
  sem_init(&connections_sem, 0, 1);
  int arg_led_count = argc - optind;
  if (arg_led_count <= 0) {
    LOG_ERROR("at least one LED name must be specified");
    print_usage(argv[0]);
    return 1;
  }

  if (host_leds_check_exist(&argv[optind], arg_led_count, LED_ROLE_NORMAL) ==
      0) {
    LOG_ERROR("No LED interfaces exist under %s, exiting", LEDS_SYSFS_BASE);
    return 0;
  }

  signal(SIGPIPE, SIG_IGN);
  fprintf(stderr,
          "Starting host-led-proxy on vsock port %u with %d LED(s) handled:",
          vsock_port, led_count);
  for (int i = 0; i < led_count; i++)
    fprintf(stderr, " %s", leds[i].name);

  fprintf(stderr, "\n");

  int ret = host_leds_run(vsock_port, allowed_cid);
  return ret;
}
