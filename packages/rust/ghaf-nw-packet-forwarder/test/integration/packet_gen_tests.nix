# SPDX-FileCopyrightText: 2022-2026 TII (SSRC) and the Ghaf contributors
# SPDX-License-Identifier: Apache-2.0
{ pkgs, crane, ... }:
let
  nw-packet-forwarder = pkgs.callPackage ../../default.nix { inherit crane; };
  packet-gener = pkgs.python3Packages.callPackage ./packages/packet-generator/derivation.nix { };

  users.users.ghaf = {
    password = "ghaf";
    isNormalUser = true;
    extraGroups = [ "wheel" ];
  };
  security.sudo.extraRules = [
    {
      groups = [ "wheel" ];
      commands = [
        {
          command = "ALL";
          options = [ "NOPASSWD" ];
        }
      ];
    }
  ];
  internalVMMac = "DE:AD:BE:EF:00:02";

  routerBase = pkgs.lib.mkMerge [
    {
      virtualisation.vlans = [
        2
        1
      ];
      networking.firewall.enable = false;
      networking.firewall.filterForward = false;
      networking.nftables.enable = false;
      networking.nat.internalIPs = [ "192.168.1.0/24" ];
      networking.nat.externalInterface = "eth1";
      networking.useDHCP = false;
      networking.interfaces."eth1".macAddress = pkgs.lib.mkForce "02:AD:00:00:00:01";
      networking.interfaces."eth2".macAddress = pkgs.lib.mkForce "DE:AD:BE:EF:00:01";
      networking.enableIPv6 = false;
    }
  ];
in
{
  name = "Network test guest-to-guest";

  nodes = {
    internalVM =
      { nodes, ... }:
      pkgs.lib.mkMerge [
        {
          inherit users security;
          networking.firewall.enable = false;
          networking.useDHCP = false;

          virtualisation.vlans = [ 1 ];
          networking.defaultGateway =
            (pkgs.lib.head nodes.netVM.networking.interfaces.eth2.ipv4.addresses).address;
          networking.nftables.enable = false;
          networking.interfaces."eth1".macAddress = pkgs.lib.mkForce internalVMMac;
          networking.enableIPv6 = false;

          environment.systemPackages = [
            pkgs.tshark
            pkgs.tcpdump
          ];
        }
      ];

    netVM =
      _:
      pkgs.lib.mkMerge [
        routerBase
        {
          inherit users security;
          networking.nat.enable = true;
          environment.systemPackages = [
            nw-packet-forwarder
            pkgs.tshark
            pkgs.tcpdump
          ];
        }
      ];

    externalVM = _: {
      inherit users security;

      virtualisation.vlans = [ 2 ];
      networking.firewall.enable = false;
      networking.useDHCP = false;
      networking.enableIPv6 = false;
      environment.systemPackages = [
        packet-gener
        pkgs.tshark
      ];
    };
  };
  testScript =
    _:
    let
      checksum_printout = "grep -q . && printf 'Bad checksum detected' || printf 'No bad checksum detected'";
      udp_checksum_tshark = "tshark -o 'udp.check_checksum:TRUE' -Y 'udp.checksum.bad' -r ";
      netvm_udp_checksum = "${udp_checksum_tshark} netvm_capture.pcap | ${checksum_printout}";
      internal_vm_checksum = "${udp_checksum_tshark} internalvm_capture.pcap  | ${checksum_printout}";
      nw-pckt-fwd = "${nw-packet-forwarder}/bin/nw-pckt-fwd --external-iface eth1 --internal-iface eth2 --internal-ip 192.168.1.3 --ccastvm-mac ${internalVMMac} --ccastvm-ip 192.168.1.2/24 --log-level debug";
      mdns_flood_tshark = "tshark -o 'udp.check_checksum:TRUE' -Y 'mdns && !udp.checksum.bad' -r ";
      netvm_mdns_flood = "${mdns_flood_tshark} netvm_capture.pcap | wc -l";
      internal_vm_mdns_flood = "${mdns_flood_tshark} internalvm_capture.pcap | wc -l";
      num_packets_mdns_flood = 1500;
      max_packet_per_window = 5;
      flood_time_in_s = num_packets_mdns_flood * 1.0e-2;
      window_in_s = 1;
      num_passed_packets_to_internal_vm = (max_packet_per_window * flood_time_in_s) / window_in_s;
    in
    ''
      start_all()
      externalVM.wait_for_unit("default.target")
      netVM.wait_for_unit("default.target")
      internalVM.wait_for_unit("default.target")

      # Start packet capture using tshark in the background
      netVM.execute("sudo tcpdump -i eth1 -Uvvv  -w netvm_capture.pcap > /dev/null 2>&1 &")
      internalVM.execute("sudo tcpdump -i eth1 -Uvvv  -w internalvm_capture.pcap > /dev/null 2>&1 &")

      netVM.sleep(1)

      netVM.execute("${nw-pckt-fwd} > /dev/null 2>&1 &")
      externalVM.sleep(1)

      # Run mDNS flood script on externalVM
      externalVM.wait_until_succeeds("sudo mdns-flood.py -c ${toString num_packets_mdns_flood}")
      externalVM.sleep(1)

      externalVM.wait_until_succeeds("sudo bad-checksum.py")

      # Give some time to capture any remaining packets
      externalVM.sleep(1)

      # Stop tcpdump after capturing completes
      netVM.execute("sudo pkill -9 tcpdump")
      internalVM.execute("sudo pkill -9 tcpdump")

      externalVM.sleep(1)
      internalVM.sleep(1)


      # Copy analysis results from VM to host
      netVM.copy_from_vm("netvm_capture.pcap", "test/integration")
      internalVM.copy_from_vm("internalvm_capture.pcap", "test/integration")

      # bad checksum validation
      netvm_bad_checksum = netVM.succeed("${netvm_udp_checksum}")
      internalvm_bad_checksum = internalVM.succeed("${internal_vm_checksum}")
      netvm_bad_checksum_log =  netVM.execute("journalctl -t nw-packet-forwarder | grep \"Wrong udp checksum\" | wc -l")

      # Validate results (Fail test if unexpected result is found)
      assert netvm_bad_checksum == "Bad checksum detected", "Expected bad checksum in netVM capture"
      assert internalvm_bad_checksum == 'No bad checksum detected', "Expected no bad checksum in internalVM capture"
      assert int(netvm_bad_checksum_log[1]) == 10, "Expected different wrong checksum packet sent"

      # rate limiting validation
      netvm_mdns_flood = netVM.succeed("${netvm_mdns_flood}")
      internalvm_mdns_flood = internalVM.succeed("${internal_vm_mdns_flood}")

      assert int(netvm_mdns_flood) == int(${toString num_packets_mdns_flood}), "Number of flooded mdns packets must be ${toString num_packets_mdns_flood}"
      print(f"internalvm_mdns_flood: {int(internalvm_mdns_flood)}")
      assert int(internalvm_mdns_flood) <= int(${toString num_passed_packets_to_internal_vm}) , "Number of allowed mdns packets in internalvm must be lower than max expected val"
      assert int(internalvm_mdns_flood) >= int(${
        toString (num_passed_packets_to_internal_vm * 0.9)
      }) , "Number of allowed mdns packets in internalvm must be at least %90 expected"

      # Stop packet forwarding
      netVM.execute("sudo pkill -9 nw-pckt-fwd")


    '';
}
