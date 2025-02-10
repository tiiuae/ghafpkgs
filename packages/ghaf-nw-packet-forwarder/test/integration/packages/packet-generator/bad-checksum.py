#!/usr/bin/env python3
# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
from scapy.all import get_if_addr, IP, UDP, DNS, DNSQR, DNSRR, send


def main():
    interface = "eth1"  # Replace with your network interface (e.g., eth0, wlan0)
    victim_ip = get_if_addr(interface)  # Get the source IP of the interface

    # mDNS response
    mdns_response_bad_checksum = (
        IP(src=victim_ip, dst="224.0.0.251")
        / UDP(sport=5353, dport=5353, chksum=0x1234)
        / DNS(
            qr=1,  # Response (qr = 1)
            aa=1,  # Authoritative Answer
            rd=1,  # Recursion Desired
            qdcount=1,  # 1 question in the query section
            ancount=1,  # 1 answer in the response section
            qd=DNSQR(
                qname="_services._dns-sd._udp.local", qtype="PTR"
            ),  # Question section
            an=DNSRR(rrname="_services._dns-sd._udp.local", rdata="192.168.2.100"),
        )
    )
    # Force a bad checksum by setting it to an incorrect value
    send(mdns_response_bad_checksum, count=10, loop=0, inter=0.1)


if __name__ == "__main__":
    main()
