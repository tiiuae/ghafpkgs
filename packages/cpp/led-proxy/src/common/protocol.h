/*
 Copyright 2026 TII (CRC) and the Ghaf contributors
 SPDX-License-Identifier: Apache-2.0
 */

#ifndef COMMON_PROTOCOL_H
#define COMMON_PROTOCOL_H
#include <stdint.h>

#define LED_PROXY_VERSION 1
#define LED_NAME_LEN (64)
#define MAX_LEDS (16)

#define LED_PROXY_VSOCK_PORT 9999

enum led_msg_type {
  LED_MSG_HELLO = 1,
  LED_MSG_ADD_LED,
  LED_MSG_SET_BRIGHTNESS,
  LED_MSG_EXIT,
};

struct led_msg_hdr {
  uint16_t version;
  uint16_t type;
  union {
    struct {
      uint16_t length;
      uint16_t max_brightness;
    };
    struct {
      uint8_t led_index;
      uint8_t brightness;
      uint8_t reserved[2];
    };
  };
};
_Static_assert(sizeof(struct led_msg_hdr) == 8,
               "led_msg_hdr size must be 8 bytes");
#endif // COMMON_PROTOCOL_H
