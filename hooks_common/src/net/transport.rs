use std::{mem, ptr, slice};
use std::ffi::CString;

use libc::c_void;

use enet_sys::{ENetEvent, ENetEventType, ENET_HOST_ANY};
use enet_sys::address::{enet_address_set_host, ENetAddress};
use enet_sys::peer::{enet_peer_send, ENetPeer};
use enet_sys::host::{enet_host_broadcast, enet_host_connect, enet_host_create, enet_host_destroy,
                     enet_host_flush, enet_host_service, ENetHost};
use enet_sys::packet::{enet_packet_create, enet_packet_destroy, ENetPacket, ENetPacketFlag};

// TODO: Error handling (check ENet return values)
// TODO: Annotate with lifetimes?

struct Address(ENetAddress);
struct Peer(*mut ENetPeer);
struct Host(*mut ENetHost);
struct Packet(*mut ENetPacket);
struct ReceivedPacket(*mut ENetPacket);

enum Event {
    Connect(Peer),
    Receive(Peer, u8, ReceivedPacket),
    Disconnect(Peer),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum PacketFlag {
    Reliable,
    Unreliable,
    Unsequenced,
}

impl Address {
    pub fn create(host: &str, port: u16) -> Address {
        let mut address = ENetAddress {
            host: 0,
            port: port,
        };

        let c_host = CString::new(host).unwrap();

        unsafe {
            enet_address_set_host(&mut address, c_host.as_ptr());
        }

        Address(address)
    }
}

impl Host {
    pub fn create_server(
        port: u16,
        peer_count: usize,
        channel_limit: usize,
        incoming_bandwidth: u32,
        outgoing_bandwidth: u32,
    ) -> Host {
        let address = ENetAddress {
            host: ENET_HOST_ANY,
            port: port,
        };

        let host = unsafe {
            enet_host_create(
                &address,
                peer_count,
                channel_limit,
                incoming_bandwidth,
                outgoing_bandwidth,
            )
        };

        Host(host)
    }

    pub fn create_client(
        channel_limit: usize,
        incoming_bandwidth: u32,
        outgoing_bandwidth: u32,
    ) -> Host {
        let host = unsafe {
            enet_host_create(
                ptr::null(),
                1,
                channel_limit,
                incoming_bandwidth,
                outgoing_bandwidth,
            )
        };

        Host(host)
    }

    pub fn connect(&self, address: Address, channel_count: usize) -> Peer {
        let peer = unsafe { enet_host_connect(self.0, &address.0, channel_count, 0) };

        Peer(peer)
    }

    pub fn service(&self, timeout_ms: u32) -> Option<Event> {
        let mut event: ENetEvent;

        unsafe {
            event = mem::uninitialized();
            enet_host_service(self.0, &mut event, timeout_ms);
        }

        match event._type {
            ENetEventType::ENET_EVENT_TYPE_NONE => None,
            ENetEventType::ENET_EVENT_TYPE_CONNECT => Some(Event::Connect(Peer(event.peer))),
            ENetEventType::ENET_EVENT_TYPE_RECEIVE => Some(Event::Receive(
                Peer(event.peer),
                event.channelID,
                ReceivedPacket(event.packet),
            )),
            ENetEventType::ENET_EVENT_TYPE_DISCONNECT => Some(Event::Disconnect(Peer(event.peer))),
        }
    }

    pub fn flush(&self) {
        unsafe {
            enet_host_flush(self.0);
        }
    }

    pub fn broadcast(&self, channel_id: u8, packet: Packet) {
        unsafe {
            enet_host_broadcast(self.0, channel_id, packet.0);
        }
    }
}

impl Peer {
    pub fn send(&self, channel_id: u8, packet: Packet) {
        unsafe {
            enet_peer_send(self.0, channel_id, packet.0);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            enet_host_destroy(self.0);
        }
    }
}

impl Packet {
    pub fn create(data: &[u8], flag: PacketFlag) -> Packet {
        let flags = match flag {
            PacketFlag::Reliable => ENetPacketFlag::ENET_PACKET_FLAG_RELIABLE as u32,
            PacketFlag::Unreliable => 0, // TODO: Check
            PacketFlag::Unsequenced => ENetPacketFlag::ENET_PACKET_FLAG_UNSEQUENCED as u32,
        };

        let packet =
            unsafe { enet_packet_create(data.as_ptr() as *const c_void, data.len(), flags) };

        Packet(packet)
    }
}

impl ReceivedPacket {
    pub fn data<'a>(&'a self) -> &'a [u8] {
        unsafe { slice::from_raw_parts((*self.0).data, (*self.0).dataLength) }
    }
}

impl Drop for ReceivedPacket {
    fn drop(&mut self) {
        unsafe {
            enet_packet_destroy(self.0);
        }
    }
}
