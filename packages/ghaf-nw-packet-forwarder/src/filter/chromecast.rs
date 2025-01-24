/*
    Copyright 2025 TII (SSRC) and the contributors
    SPDX-License-Identifier: Apache-2.0
*/
use crate::cli;
use crate::forward_impl::forward::Ifaces;
use log::{debug, info};
use pnet::ipnetwork::IpNetwork;
use pnet::packet::dns::DnsPacket;
use pnet::packet::ethernet::EthernetPacket;
use pnet::packet::ip::IpNextHeaderProtocols;
use pnet::packet::ipv4::Ipv4Packet;
use pnet::packet::udp::UdpPacket;
use pnet::packet::Packet;
use pnet::util::MacAddr;
use std::collections::VecDeque;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::UdpSocket;
use std::sync::Arc;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
const SSDP_MULTICAST_ADDR: Ipv4Addr = Ipv4Addr::new(239, 255, 255, 250);
const SSDP_PORT: u16 = 1900;
const MAX_SSDP_PORTS: usize = 3;
const MAX_DURATION: Duration = Duration::new(5, 0); // 3 seconds

const MDNS_PORT: u16 = 5353;
const MDNS_IP: Ipv4Addr = Ipv4Addr::new(224, 0, 0, 251);
const MDNS_MAC: MacAddr = MacAddr(0x01, 0x0, 0x5E, 0x0, 0x0, 0xFB);

const SSDP_MAC: MacAddr = MacAddr(0x01, 0x0, 0x5E, 0x7F, 0xFF, 0xFA);

pub struct Chromecast {
    //shared_data: Arc<SharedData>,
    external_ops: Arc<ExternalOps>,
    internal_ops: Arc<InternalOps>,
}

impl Chromecast {
    /// Creates a new `Chromecast` instance, initializing the shared data, external, and internal operations.
    ///
    /// # Arguments
    ///
    /// * `ifaces` - An `Ifaces` struct containing information about the interfaces (e.g., IP addresses).
    ///
    /// # Returns
    ///
    /// Returns a new `Chromecast` instance that is initialized with the provided
    /// interface information and the necessary operations for interacting with it.
    pub fn new(ifaces: Ifaces) -> Self {
        let shared_data = Arc::new(SharedData::new(
            cli::get_chromecast(),
            cli::get_chromevm_ip(),
            cli::get_chromevm_mac(),
            false,
            true,
        )); // Ensure shared_data is wrapped in Arc

        let external_ops = Arc::new(ExternalOps::new(shared_data.clone()));
        let internal_ops = Arc::new(InternalOps::new(shared_data.clone()));

        let interface = "0.0.0.0"; // Use the appropriate interface address or "0.0.0.0" for default.

        // Create a UDP socket and bind it to the local interface.
        let socket = UdpSocket::bind(format!("{interface}:{}", 0)).expect("Failed to bind socket");
        Self::join_multicast_groups(&socket, &ifaces.ext_ip);
        Self::join_multicast_groups(&socket, &ifaces.int_ip);

        Self {
            external_ops,
            internal_ops,
        }
    }

    fn join_multicast_groups(socket: &UdpSocket, ip: &IpNetwork) {
        if let IpAddr::V4(ipv4) = ip.ip() {
            socket
                .join_multicast_v4(&SSDP_MULTICAST_ADDR, &ipv4)
                .unwrap_or_else(|_| panic!("Failed to join multicast group ssdp for IP: {}", ipv4));

            socket
                .join_multicast_v4(&MDNS_IP, &ipv4)
                .unwrap_or_else(|_| panic!("Failed to join multicast group mdns for IP: {}", ipv4));
        }
    }
    /// Returns a reference to the external operations instance (`ExternalOps`) wrapped in an `Arc`.
    ///
    /// This function allows external code to access the operations related to the Chromecast in the external network.
    ///
    /// # Returns
    ///
    /// Returns an `Arc<ExternalOps>`, which can be used to interact with the external network-related operations of the `Chromecast`.
    pub fn get_external_ops(&self) -> Arc<ExternalOps> {
        self.external_ops.clone() // No need to lock here, just return Arc for safe sharing
    }

    pub fn get_internal_ops(&self) -> Arc<InternalOps> {
        self.internal_ops.clone() // No need to lock here, just return Arc for safe sharing
    }
}

struct SharedData {
    enabled: bool,
    ssdp_ports: Mutex<VecDeque<(u16, SystemTime)>>, // Thread-safe vector of ports
    ip: IpNetwork,
    mac: MacAddr,
    ssdp_enabled: bool,
    mdns_enabled: bool,
}
impl SharedData {
    fn new(
        enabled: bool,
        ip: IpNetwork,
        mac: MacAddr,
        ssdp_enabled: bool,
        mdns_enabled: bool,
    ) -> Self {
        SharedData {
            enabled,
            ssdp_ports: Mutex::new(VecDeque::with_capacity(MAX_SSDP_PORTS)),
            ip,
            mac,
            ssdp_enabled,
            mdns_enabled,
        }
    }

    fn get_enabled(&self) -> bool {
        self.enabled
    }

    async fn add_ssdp_port(&self, port: u16) {
        let mut ports_lock = self.ssdp_ports.lock().await;

        // Remove the port if it already exists
        ports_lock.retain(|&(stored_port, _)| stored_port != port);

        // Add the new port
        if ports_lock.len() >= MAX_SSDP_PORTS {
            ports_lock.pop_front();
        }
        ports_lock.push_back((port, SystemTime::now()));

        info!("SSDP Port map: {:?}", ports_lock);
    }

    async fn is_ssdp_port_available(&self, port: u16) -> bool {
        let ports_lock = self.ssdp_ports.lock().await;
        let now = SystemTime::now();

        for &(stored_port, timestamp) in ports_lock.iter() {
            if stored_port == port {
                if let Ok(duration) = now.duration_since(timestamp) {
                    return duration <= MAX_DURATION;
                }
            }
        }

        false
    }

    fn get_ip(&self) -> IpNetwork {
        self.ip
    }
    fn get_mac(&self) -> MacAddr {
        self.mac
    }
}

pub struct ExternalOps {
    shared_data: Arc<SharedData>, // Shared data with thread-safe access
}

impl ExternalOps {
    fn new(shared_data: Arc<SharedData>) -> Self {
        Self { shared_data }
    }
    /// Determines if the given Ethernet packet is an external-to-internal packet for Chromecast.
    ///
    /// # Arguments
    ///
    /// * `eth_packet` - The Ethernet packet to check.
    ///
    /// # Returns
    ///
    /// Returns `Some((MacAddr, IpNetwork))` of `chrome-VM` in ghaf if the packet matches external-to-internal criteria, otherwise `None`.
    ///
    /// # Example
    ///
    /// ```
    /// let eth_packet = EthernetPacket::new(&packet_data).unwrap();
    /// let result = external_ops.is_ext_to_int_packet(&eth_packet).await;
    /// assert_eq!(result, Some((mac_address, ip_network)));
    /// ```
    pub async fn is_ext_to_int_packet(
        &self,
        eth_packet: &EthernetPacket<'_>,
    ) -> Option<(MacAddr, IpNetwork)> {
        let enabled = self.shared_data.get_enabled(); // Fixed: Call `get_enabled` instead of direct access
        if !enabled {
            return None;
        }
        let ip = self.shared_data.get_ip();
        let mac = self.shared_data.get_mac();

        let ssdp_enabled = self.shared_data.ssdp_enabled;
        let mdns_enabled = self.shared_data.mdns_enabled;
        if let Some(ipv4_packet) = Ipv4Packet::new(eth_packet.payload()) {
            if ipv4_packet.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
                if let Some(udp_packet) = UdpPacket::new(ipv4_packet.payload()) {
                    let dest_port = udp_packet.get_destination();
                    let dest_ip = ipv4_packet.get_destination();
                    let src_ip = ipv4_packet.get_source();
                    if self.shared_data.is_ssdp_port_available(dest_port).await {
                        info!(
                            "Ext to Int - Chromecast udp packet detected,port num: {}",
                            dest_port
                        );
                        return Some((mac, ip));
                    } else if mdns_enabled && dest_port == MDNS_PORT && dest_ip == MDNS_IP {
                        let is_mdns_response = self.is_mdns_response(udp_packet.payload());
                        info!(
                            "Ext to Int - mdns packet detected,src ip: {}, response: {}",
                            src_ip, is_mdns_response
                        );
                        if is_mdns_response {
                            return Some((
                                MDNS_MAC,
                                IpNetwork::new(std::net::IpAddr::V4(MDNS_IP), 32).unwrap(),
                            ));
                        }
                    } else if ssdp_enabled
                        && dest_ip == SSDP_MULTICAST_ADDR
                        && dest_port == SSDP_PORT
                    {
                        info!("Ext to Int - ssdp packet fowarded to internal interface");
                        return Some((
                            SSDP_MAC,
                            IpNetwork::new(std::net::IpAddr::V4(SSDP_MULTICAST_ADDR), 32).unwrap(),
                        ));
                    }
                }
            }
        }

        None
    }

    fn is_mdns_response(&self, udp_payload: &[u8]) -> bool {
        // Parse the UDP payload as an mDNS message
        if let Some(dns_message) = DnsPacket::new(udp_payload) {
            // Filter based on queries
            if 1 == dns_message.get_is_response() {
                return true;
            }
        }
        false
    }

    // Add more external operations here as needed
}

pub struct InternalOps {
    shared_data: Arc<SharedData>, // Shared data with thread-safe access
}

impl InternalOps {
    fn new(shared_data: Arc<SharedData>) -> Self {
        Self { shared_data }
    }
    /// Filters packets from internal to external network for chromecast, checking if they match specific criteria like SSDP and mDNS.
    ///
    /// This function checks if the packet is related to SSDP or mDNS, and whether it should be forwarded based on specific conditions. It ensures the packet originates from the correct internal IP and applies additional filtering for UDP packets.
    ///
    /// # Arguments
    ///
    /// * `eth_packet` - The Ethernet packet to be checked. This packet is parsed to extract the payload and determine whether it matches the filtering criteria.
    ///
    /// # Returns
    ///
    /// Returns `true` if the packet matches the internal-to-external forwarding criteria, and `false` otherwise.
    ///
    /// # Notes
    ///
    /// This function checks for the following conditions:
    /// - The packet's source IP must match the internal IP address of the `chrome VM`.
    /// - The packet must be a UDP packet with either an SSDP or mDNS destination port.
    /// - It supports filtering based on mDNS queries and responses and SSDP packets.
    ///
    /// # Example
    ///
    /// ```
    /// let eth_packet = EthernetPacket::new(&packet_data).unwrap();
    /// let result = internal_ops.int_to_ext_filter_packets(&eth_packet).await;
    /// assert!(result);
    /// ```
    pub async fn int_to_ext_filter_packets(&self, eth_packet: &EthernetPacket<'_>) -> bool {
        let enabled = self.shared_data.get_enabled();
        if !enabled {
            return false;
        }
        let ssdp_enabled = self.shared_data.ssdp_enabled;
        let mdns_enabled = self.shared_data.mdns_enabled;

        if let Some(ipv4_packet) = Ipv4Packet::new(eth_packet.payload()) {
            let src_ip = ipv4_packet.get_source();
            let chrome_vm_ip = self.shared_data.get_ip();

            if src_ip != self.shared_data.get_ip().ip() {
                return false;
            }
            if ipv4_packet.get_next_level_protocol() == IpNextHeaderProtocols::Udp {
                if let Some(udp_packet) = UdpPacket::new(ipv4_packet.payload()) {
                    let dest_ip = ipv4_packet.get_destination();
                    let dest_port = udp_packet.get_destination();
                    if dest_ip == SSDP_MULTICAST_ADDR && dest_port == SSDP_PORT {
                        let src_port = udp_packet.get_source();
                        self.shared_data.add_ssdp_port(src_port).await;
                        debug!("Added SSDP port {} to the list of ports", src_port);
                        return ssdp_enabled;
                    } else if mdns_enabled
                        && src_ip == chrome_vm_ip.ip()
                        && dest_port == MDNS_PORT
                        && dest_ip == MDNS_IP
                    {
                        let is_mdns_query = self.is_mdns_query(udp_packet.payload());
                        info!(
                            "Int to Ext - mdns packet detected, src ip: {}, query:{}",
                            src_ip, is_mdns_query
                        );
                        return is_mdns_query;
                    }
                }
            }
        }
        false
    }

    fn is_mdns_query(&self, udp_payload: &[u8]) -> bool {
        // Parse the UDP payload as an mDNS message
        if let Some(dns_message) = DnsPacket::new(udp_payload) {
            // Filter based on queries
            if 0 == dns_message.get_is_response() {
                return true;
            }
        }
        false
    }

    // Add more external operations here as needed
}
