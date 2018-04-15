//! Transport wrapper for reading and writing in a background thread.

use std::collections::BTreeMap;
use std::mem;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use net::transport::{self, ChannelId, Event, Packet, PacketFlag, PeerId};

#[derive(Debug)]
pub enum Error {
    SendError,
    RecvError,
}

pub trait PeerData: Send + Default + 'static {
    fn receive<H: transport::Host>(
        &mut self,
        host: &mut H,
        peer_id: PeerId,
        channel_id: ChannelId,
        data: &[u8],
    ) -> Result<bool, H::Error>;
}

impl PeerData for () {
    fn receive<H: transport::Host>(
        &mut self,
        _: &mut H,
        _: PeerId,
        _: ChannelId,
        _: &[u8],
    ) -> Result<bool, H::Error> {
        Ok(false)
    }
}

pub struct Host<H, D>
where
    H: transport::Host,
    D: PeerData,
{
    sender: Sender<Command>,
    receiver: Receiver<Event<H::Packet>>,
    peers: Peers<D>,
    thread: Option<thread::JoinHandle<()>>,
}

enum Command {
    Send(PeerId, ChannelId, PacketFlag, Vec<u8>),
    Disconnect(PeerId, u32),
    Stop,
}

type Peers<D> = Arc<Mutex<BTreeMap<PeerId, D>>>;

impl<H: transport::Host, D> transport::Host for Host<H, D>
where
    H: transport::Host,
    D: PeerData,
{
    type Error = Error;
    type Packet = H::Packet;

    fn is_peer(&self, id: PeerId) -> bool {
        let peers = self.peers.lock().unwrap();
        peers.contains_key(&id)
    }

    fn service(&mut self, timeout_ms: u32) -> Result<Option<Event<H::Packet>>, Error> {
        match self.receiver
            .recv_timeout(Duration::from_millis(timeout_ms as u64))
        {
            Ok(event) => {
                match &event {
                    &Event::Disconnect(peer_id) => {
                        let mut peers = self.peers.lock().unwrap();
                        peers.remove(&peer_id);
                    }
                    _ => {}
                }

                Ok(Some(event))
            }
            Err(RecvTimeoutError::Timeout) => {
                // Ignore empty channel
                Ok(None)
            }
            Err(RecvTimeoutError::Disconnected) => Err(Error::RecvError),
        }
    }

    fn flush(&mut self) -> Result<(), Error> {
        // noop
        Ok(())
    }

    fn send(
        &mut self,
        peer_id: PeerId,
        channel_id: ChannelId,
        flag: PacketFlag,
        data: &[u8],
    ) -> Result<(), Self::Error> {
        self.sender
            .send(Command::Send(peer_id, channel_id, flag, data.to_vec()))
            .map_err(|_| Error::SendError)
    }

    fn disconnect(&mut self, peer_id: PeerId, data: u32) -> Result<(), Self::Error> {
        self.sender
            .send(Command::Disconnect(peer_id, data))
            .map_err(|_| Error::SendError)
    }
}

impl<H, D> Drop for Host<H, D>
where
    H: transport::Host,
    D: PeerData,
{
    fn drop(&mut self) {
        let thread = mem::replace(&mut self.thread, None);
        if let Some(thread) = thread {
            if let Ok(_) = self.sender.send(Command::Stop) {
                if let Err(err) = thread.join() {
                    warn!("Failed to join background thread: {:?}", err);
                }
            } else {
                warn!("Could not send stop command to background thread");
            }
        }
    }
}

impl<H, D> Host<H, D>
where
    H: transport::Host + Send + 'static,
    <H as transport::Host>::Packet: Send + 'static,
    D: PeerData,
{
    pub fn spawn(host: H) -> Host<H, D> {
        let (sender_command, receiver_command) = channel();
        let (sender_event, receiver_event) = channel();
        let peers = Arc::new(Mutex::new(BTreeMap::new()));

        let thread_peers = peers.clone();
        let builder = thread::Builder::new()
            .name("hooks_common::net::transport::async::background_thread".to_string());
        let thread = builder
            .spawn(move || {
                background_thread(receiver_command, sender_event, thread_peers, host);
            })
            .unwrap();

        Host {
            sender: sender_command,
            receiver: receiver_event,
            peers: peers,
            thread: Some(thread),
        }
    }

    pub fn peers(&self) -> Peers<D> {
        self.peers.clone()
    }
}

fn background_thread<H, D>(
    receiver: Receiver<Command>,
    sender: Sender<Event<H::Packet>>,
    peers: Peers<D>,
    mut host: H,
) where
    H: transport::Host,
    D: PeerData,
{
    loop {
        thread::yield_now();

        match receiver.try_recv() {
            Ok(Command::Send(peer_id, channel_id, packet_flag, data)) => {
                if host.is_peer(peer_id) {
                    // FIXME: Propagate errors to main thread
                    host.send(peer_id, channel_id, packet_flag, &data).unwrap();
                } else {
                    // The peer might already have been removed asynchronously
                    debug!("Ignoring send command to invalid peer_id {}", peer_id);
                }
            }
            Ok(Command::Disconnect(peer_id, data)) => {
                if host.is_peer(peer_id) {
                    // FIXME: Propagate errors to main thread
                    host.disconnect(peer_id, data).unwrap();
                } else {
                    // The peer might already have been removed asynchronously
                    debug!("Ignoring disconnect command to invalid peer_id {}", peer_id);
                }
            }
            Ok(Command::Stop) => {
                return;
            }
            Err(TryRecvError::Empty) => {
                // Ignore empty channel
            }
            Err(TryRecvError::Disconnected) => {
                // This should only happen while faultily shutting down -- just ignore
                return;
            }
        }

        match host.service(0) {
            Ok(Some(event)) => {
                let no_send = match &event {
                    &Event::Connect(peer_id) => {
                        let mut peers = peers.lock().unwrap();
                        peers.insert(peer_id, Default::default());
                        false
                    }
                    &Event::Receive(peer_id, channel_id, ref packet) => {
                        // FIXME: Propagate errors to main thread
                        let no_send = {
                            let mut peers = peers.lock().unwrap();
                            peers.get_mut(&peer_id).unwrap().receive(
                                &mut host,
                                peer_id,
                                channel_id,
                                packet.data(),
                            )
                        };
                        no_send.expect("User data receive failed")
                    }
                    _ => false,
                };

                if !no_send {
                    if let Err(_) = sender.send(event) {
                        // This should only happen while faultily shutting down -- just ignore
                        return;
                    }
                }
            }
            Ok(None) => {
                // Ignore no event
            }
            Err(error) => {
                // FIXME: Propagate errors to main thread
                panic!("Service error: {:?}", error);
            }
        }
    }
}
