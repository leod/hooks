use std::collections::{btree_map, BTreeMap};
use std::ffi::CString;
use std::os::raw::c_void;
use std::{mem, ptr, slice};

use enet_sys::{enet_address_set_host, enet_host_connect, enet_host_create, enet_host_destroy,
               enet_host_flush, enet_host_service, enet_packet_create, enet_packet_destroy,
               enet_peer_disconnect, enet_peer_send, ENetAddress, ENetEvent, ENetHost, ENetPacket,
               ENetPeer, _ENetEventType_ENET_EVENT_TYPE_CONNECT,
               _ENetEventType_ENET_EVENT_TYPE_DISCONNECT, _ENetEventType_ENET_EVENT_TYPE_NONE,
               _ENetEventType_ENET_EVENT_TYPE_RECEIVE, _ENetPacketFlag_ENET_PACKET_FLAG_RELIABLE,
               _ENetPacketFlag_ENET_PACKET_FLAG_UNSEQUENCED, ENET_HOST_ANY};

use net::transport::{self, ChannelId, Event, PacketFlag, PeerId};

#[derive(Debug)]
pub enum Error {
    InvalidPeerId(PeerId),
    InvalidEvent,
    HostNullPointer,
    ConnectNullPointer,
    PacketNullPointer,
    SendFailure(PeerId, ChannelId, PacketFlag, usize, i32),
    ServiceFailure(i32),
    AddressFailure,
}
pub struct Packet(*mut ENetPacket);
pub struct Host {
    handle: *mut ENetHost,
    next_peer_id: PeerId,
    peers: BTreeMap<PeerId, Peer>,
}

pub struct Address(ENetAddress);
struct Peer(*mut ENetPeer);

unsafe impl Send for Host {}
unsafe impl Send for Packet {}

impl transport::Host for Host {
    type Error = Error;
    type Packet = Packet;

    fn is_peer(&self, id: PeerId) -> bool {
        self.peers.contains_key(&id)
    }

    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<Packet>>, Error> {
        let mut event: ENetEvent;

        let result = unsafe {
            event = mem::uninitialized();
            enet_host_service(self.handle, &mut event, timeout_ms)
        };

        if result < 0 {
            //return Err(Error::ServiceFailure(result));
            // FIXME: Find out if this failure happens due to us using enet in the wrong way.
            warn!("enet_host_service failure: {}. ignoring.", result);
            return Ok(None);
        }

        if event.type_ == _ENetEventType_ENET_EVENT_TYPE_NONE {
            Ok(None)
        } else if event.type_ == _ENetEventType_ENET_EVENT_TYPE_CONNECT {
            if !event.peer.is_null() {
                let id = self.register_peer(event.peer);
                Ok(Some(Event::Connect(id)))
            } else {
                Err(Error::InvalidEvent)
            }
        } else if event.type_ == _ENetEventType_ENET_EVENT_TYPE_RECEIVE {
            let id = Peer(event.peer).id();
            if !event.peer.is_null() {
                if let Some(_) = self.peers.get(&id) {
                    let packet = Packet(event.packet);
                    Ok(Some(Event::Receive(id, event.channelID, packet)))
                } else {
                    Err(Error::InvalidEvent)
                }
            } else {
                Err(Error::InvalidEvent)
            }
        } else if event.type_ == _ENetEventType_ENET_EVENT_TYPE_DISCONNECT {
            if !event.peer.is_null() {
                let id = Peer(event.peer).id();
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
        } else {
            Err(Error::InvalidEvent)
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        unsafe {
            enet_host_flush(self.handle);
        }

        Ok(())
    }

    fn disconnect(&mut self, peer_id: PeerId, data: u32) -> Result<(), Error> {
        let peer = self.peers
            .get(&peer_id)
            .ok_or(Error::InvalidPeerId(peer_id))?;

        unsafe {
            enet_peer_disconnect(peer.0, data);
        }

        Ok(())
    }

    fn send(
        &mut self,
        peer_id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), Error> {
        let peer = self.peers
            .get(&peer_id)
            .ok_or(Error::InvalidPeerId(peer_id))?;
        peer.send(channel_id, flag, data)
    }
}

impl Peer {
    fn set_id(&mut self, id: PeerId) {
        unsafe {
            (*self.0).data = id as *mut c_void;
        }
    }

    fn id(&self) -> PeerId {
        unsafe { (*self.0).data as PeerId }
    }

    fn send(&self, channel_id: ChannelId, flag: PacketFlag, data: &[u8]) -> Result<(), Error> {
        let flags = match flag {
            PacketFlag::Reliable => _ENetPacketFlag_ENET_PACKET_FLAG_RELIABLE as u32,
            PacketFlag::Unreliable => 0, // TODO: Check
            PacketFlag::Unsequenced => _ENetPacketFlag_ENET_PACKET_FLAG_UNSEQUENCED as u32,
        };

        // NOTE: `enet_packet_create` copies the given data, so we don't need to make sure that the
        //       data lives as long as the created package.
        let packet =
            unsafe { enet_packet_create(data.as_ptr() as *const c_void, data.len(), flags) };
        if packet.is_null() {
            return Err(Error::PacketNullPointer);
        }

        let result = unsafe { enet_peer_send(self.0, channel_id, packet) };
        if result != 0 {
            if flag == PacketFlag::Reliable {
                return Err(Error::SendFailure(
                    self.id(),
                    channel_id,
                    flag,
                    data.len(),
                    result,
                ));
            } else {
                // FIXME: Find out if this failure happens due to us using enet in the wrong way.
                warn!(
                    "enet_peer_send failure: {}. ignoring for unreliable packet.",
                    result
                );
            }
        }

        Ok(())
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

        if !handle.is_null() {
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

        if !handle.is_null() {
            Ok(Host::new(handle))
        } else {
            Err(Error::HostNullPointer)
        }
    }

    pub fn connect<'a>(&'a mut self, address: &Address, channel_count: usize) -> Result<(), Error> {
        let handle = unsafe { enet_host_connect(self.handle, &address.0, channel_count, 0) };

        if !handle.is_null() {
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
