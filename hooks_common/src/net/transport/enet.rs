use std::collections::{btree_map, BTreeMap};
use std::ffi::CString;
use std::{mem, ptr, slice};

use libc::c_void;

use enet_sys::address::{enet_address_set_host, ENetAddress};
use enet_sys::host::{enet_host_connect, enet_host_create, enet_host_destroy, enet_host_flush,
                     enet_host_service, ENetHost};
use enet_sys::packet::{enet_packet_create, enet_packet_destroy, ENetPacket, ENetPacketFlag};
use enet_sys::peer::{enet_peer_disconnect, enet_peer_send, ENetPeer};
use enet_sys::{ENetEvent, ENetEventType, ENET_HOST_ANY};

use net::transport::{self, ChannelId, Event, PacketFlag, PeerId};

#[derive(Debug)]
pub struct Transport;

impl transport::Transport for Transport {
    type Error = Error;
    type Host = Host;
    type Peer = Peer;
    type Packet = Packet;
}

#[derive(Debug)]
pub enum Error {
    InvalidEvent,
    HostNullPointer,
    ConnectNullPointer,
    PacketNullPointer,
    SendFailure(PeerId, ChannelId, PacketFlag, usize, i32),
    ServiceFailure,
    AddressFailure,
}

pub struct Address(ENetAddress);

pub struct Host {
    handle: *mut ENetHost,
    next_peer_id: PeerId,
    peers: BTreeMap<PeerId, Peer>,
}

pub struct Peer(*mut ENetPeer);

pub struct Packet(*mut ENetPacket);

impl transport::Peer for Peer {
    type Transport = Transport;

    fn id(&self) -> PeerId {
        unsafe { (*self.0).data as PeerId }
    }

    fn send(&mut self, channel_id: ChannelId, flag: PacketFlag, data: &[u8]) -> Result<(), Error> {
        let flags = match flag {
            PacketFlag::Reliable => ENetPacketFlag::ENET_PACKET_FLAG_RELIABLE as u32,
            PacketFlag::Unreliable => 0, // TODO: Check
            PacketFlag::Unsequenced => ENetPacketFlag::ENET_PACKET_FLAG_UNSEQUENCED as u32,
        };

        // NOTE: `enet_packet_create` copies the given data, so we don't need to make sure that the
        //       data lives as long as the created package.
        let packet =
            unsafe { enet_packet_create(data.as_ptr() as *const c_void, data.len(), flags) };
        if packet == ptr::null_mut() {
            return Err(Error::PacketNullPointer);
        }

        let result = unsafe { enet_peer_send(self.0, channel_id, packet) };
        if result != 0 {
            return Err(Error::SendFailure(
                self.id(),
                channel_id,
                flag,
                data.len(),
                result,
            ));
        }

        Ok(())
    }

    fn disconnect(&mut self, data: u32) {
        unsafe {
            enet_peer_disconnect(self.0, data);
        }
    }
}

impl Peer {
    fn set_id(&mut self, id: PeerId) {
        unsafe {
            (*self.0).data = id as *mut c_void;
        }
    }
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
            Err(Error::AddressFailure)
        }
    }
}

impl Host {
    fn new(handle: *mut ENetHost) -> Host {
        Host {
            handle,
            next_peer_id: 1,
            peers: BTreeMap::new(),
        }
    }

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

        let handle = unsafe {
            enet_host_create(
                &address,
                peer_count,
                channel_limit,
                incoming_bandwidth,
                outgoing_bandwidth,
            )
        };

        if handle != ptr::null_mut() {
            Ok(Host::new(handle))
        } else {
            Err(Error::HostNullPointer)
        }
    }

    pub fn create_client(
        channel_limit: usize,
        incoming_bandwidth: u32,
        outgoing_bandwidth: u32,
    ) -> Result<Host, Error> {
        let handle = unsafe {
            enet_host_create(
                ptr::null(),
                1,
                channel_limit,
                incoming_bandwidth,
                outgoing_bandwidth,
            )
        };

        if handle != ptr::null_mut() {
            Ok(Host::new(handle))
        } else {
            Err(Error::HostNullPointer)
        }
    }

    pub fn connect<'a>(&'a mut self, address: &Address, channel_count: usize) -> Result<(), Error> {
        let handle = unsafe { enet_host_connect(self.handle, &address.0, channel_count, 0) };

        if handle != ptr::null_mut() {
            Ok(())
        } else {
            Err(Error::ConnectNullPointer)
        }
    }

    fn register_peer<'a>(&'a mut self, handle: *mut ENetPeer) -> PeerId {
        let peer_id = self.next_peer_id;
        self.next_peer_id += 1;

        // Make sure we don't store any duplicate peer handles
        assert!(
            !self.peers.contains_key(&peer_id),
            "PeerId used more than once"
        );
        for peer in self.peers.values() {
            assert!(peer.0 != handle, "ENetPeer handle stored more than once");
        }

        let mut peer = Peer(handle);
        peer.set_id(peer_id);
        self.peers.insert(peer_id, peer);

        peer_id
    }
}

impl transport::Host for Host {
    type Transport = Transport;

    fn get_peer<'a>(&'a mut self, id: PeerId) -> Option<&'a mut Peer> {
        self.peers.get_mut(&id)
    }

    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Transport>>, Error> {
        let mut event: ENetEvent;

        let result = unsafe {
            event = mem::uninitialized();
            enet_host_service(self.handle, &mut event, timeout_ms)
        };

        if result < 0 {
            return Err(Error::ServiceFailure);
        }

        match event._type {
            ENetEventType::ENET_EVENT_TYPE_NONE => Ok(None),
            ENetEventType::ENET_EVENT_TYPE_CONNECT => {
                if event.peer != ptr::null_mut() {
                    let id = self.register_peer(event.peer);
                    Ok(Some(Event::Connect(id)))
                } else {
                    Err(Error::InvalidEvent)
                }
            }
            ENetEventType::ENET_EVENT_TYPE_RECEIVE => {
                let id = transport::Peer::id(&Peer(event.peer));
                if event.peer != ptr::null_mut() {
                    if let Some(_) = self.get_peer(id) {
                        let packet = Packet(event.packet);
                        Ok(Some(Event::Receive(id, event.channelID, packet)))
                    } else {
                        Err(Error::InvalidEvent)
                    }
                } else {
                    Err(Error::InvalidEvent)
                }
            }
            ENetEventType::ENET_EVENT_TYPE_DISCONNECT => {
                if event.peer != ptr::null_mut() {
                    let id = transport::Peer::id(&Peer(event.peer));
                    match self.peers.entry(id) {
                        btree_map::Entry::Occupied(entry) => {
                            entry.remove();
                            Ok(Some(Event::Disconnect(id)))
                        }
                        btree_map::Entry::Vacant(_) => Err(Error::InvalidEvent),
                    }
                } else {
                    Err(Error::InvalidEvent)
                }
            }
        }
    }

    fn flush(&mut self) {
        unsafe {
            enet_host_flush(self.handle);
        }
    }
}

impl Drop for Host {
    fn drop(&mut self) {
        unsafe {
            enet_host_destroy(self.handle);
        }
    }
}

impl transport::Packet for Packet {
    fn data(&self) -> &[u8] {
        unsafe { slice::from_raw_parts((*self.0).data, (*self.0).dataLength) }
    }
}

impl Drop for Packet {
    fn drop(&mut self) {
        unsafe {
            enet_packet_destroy(self.0);
        }
    }
}
