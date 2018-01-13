use std::{mem, ptr, slice};
use std::ffi::CString;

use libc::c_void;

use enet_sys::{ENetEvent, ENetEventType, ENET_HOST_ANY};
use enet_sys::address::{enet_address_set_host, ENetAddress};
use enet_sys::host::{enet_host_broadcast, enet_host_connect, enet_host_create, enet_host_destroy,
                     enet_host_flush, enet_host_service, ENetHost};
use enet_sys::packet::{enet_packet_create, enet_packet_destroy, ENetPacket, ENetPacketFlag};
use enet_sys::peer::{enet_peer_disconnect, enet_peer_send, ENetPeer};

#[derive(Debug)]
pub enum Error {
    NullPointer,
    Failure,
}

// TODO: Annotate with lifetimes?

pub struct Address(ENetAddress);
#[derive(Clone)]
pub struct Peer(*mut ENetPeer);
pub struct Host(*mut ENetHost);
pub struct Packet(*mut ENetPacket);
pub struct ReceivedPacket(*mut ENetPacket);

pub enum Event {
    Connect(Peer),
    Receive(Peer, u8, ReceivedPacket),
    Disconnect(Peer),
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PacketFlag {
    Reliable,
    Unreliable,
    Unsequenced,
}

impl Address {
    pub fn create(host: &str, port: u16) -> Result<Address, Error> {
        let mut address = ENetAddress {
            host: 0,
            port: port,
        };

        let c_host = CString::new(host).unwrap();

        let result = unsafe { enet_address_set_host(&mut address, c_host.as_ptr()) };

        if result == 0 {
            Ok(Address(address))
        } else {
            Err(Error::Failure)
        }
    }
}

impl Host {
    pub fn create_server(
        port: u16,
        peer_count: usize,
        channel_limit: usize,
        incoming_bandwidth: u32,
        outgoing_bandwidth: u32,
    ) -> Result<Host, Error> {
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

        if host != ptr::null_mut() {
            Ok(Host(host))
        } else {
            Err(Error::NullPointer)
        }
    }

    pub fn create_client(
        channel_limit: usize,
        incoming_bandwidth: u32,
        outgoing_bandwidth: u32,
    ) -> Result<Host, Error> {
        let host = unsafe {
            enet_host_create(
                ptr::null(),
                1,
                channel_limit,
                incoming_bandwidth,
                outgoing_bandwidth,
            )
        };

        if host != ptr::null_mut() {
            Ok(Host(host))
        } else {
            Err(Error::NullPointer)
        }
    }

    pub fn connect(&self, address: &Address, channel_count: usize) -> Result<Peer, Error> {
        let peer = unsafe { enet_host_connect(self.0, &address.0, channel_count, 0) };

        if peer != ptr::null_mut() {
            Ok(Peer(peer))
        } else {
            Err(Error::NullPointer)
        }
    }

    pub fn service(&self, timeout_ms: u32) -> Result<Option<Event>, Error> {
        let mut event: ENetEvent;

        let result = unsafe {
            event = mem::uninitialized();
            enet_host_service(self.0, &mut event, timeout_ms)
        };

        if result < 0 {
            return Err(Error::Failure);
        }

        Ok(match event._type {
            ENetEventType::ENET_EVENT_TYPE_NONE => None,
            ENetEventType::ENET_EVENT_TYPE_CONNECT => Some(Event::Connect(Peer(event.peer))),
            ENetEventType::ENET_EVENT_TYPE_RECEIVE => Some(Event::Receive(
                Peer(event.peer),
                event.channelID,
                ReceivedPacket(event.packet),
            )),
            ENetEventType::ENET_EVENT_TYPE_DISCONNECT => Some(Event::Disconnect(Peer(event.peer))),
        })
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
    pub fn send(&self, channel_id: u8, packet: Packet) -> Result<(), Error> {
        let result = unsafe { enet_peer_send(self.0, channel_id, packet.0) };

        if result == 0 {
            Ok(())
        } else {
            Err(Error::Failure)
        }
    }

    pub fn data(&self) -> usize {
        unsafe { (*self.0).data as usize }
    }

    pub fn set_data(&self, n: usize) {
        unsafe {
            (*self.0).data = n as *mut c_void;
        }
    }

    pub fn disconnect(&self, data: u32) {
        unsafe {
            enet_peer_disconnect(self.0, data);
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
    pub fn create(data: &[u8], flag: PacketFlag) -> Result<Packet, Error> {
        let flags = match flag {
            PacketFlag::Reliable => ENetPacketFlag::ENET_PACKET_FLAG_RELIABLE as u32,
            PacketFlag::Unreliable => 0, // TODO: Check
            PacketFlag::Unsequenced => ENetPacketFlag::ENET_PACKET_FLAG_UNSEQUENCED as u32,
        };

        // NOTE: `enet_packet_create` copies the given data, so we don't need to make sure that the
        // data lives as long as the returned `Packet`.
        let packet =
            unsafe { enet_packet_create(data.as_ptr() as *const c_void, data.len(), flags) };

        if packet != ptr::null_mut() {
            Ok(Packet(packet))
        } else {
            Err(Error::NullPointer)
        }
    }
}

impl ReceivedPacket {
    pub fn data(&self) -> &[u8] {
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
