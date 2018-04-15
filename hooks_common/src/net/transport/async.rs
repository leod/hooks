use std::collections::BTreeMap;
use std::mem;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use net::transport::{self, ChannelId, Event, PacketFlag, PeerId};

#[derive(Debug)]
pub enum Error {
    SendError,
    RecvError,
}

/*trait PeerData: Send + 'static {
    fn receive(&mut self, host: peer_id: PeerId,
}*/

pub struct Host<H: transport::Host, D> {
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

impl<H: transport::Host, D> transport::Host for Host<H, D> {
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

impl<H: transport::Host, D> Drop for Host<H, D> {
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
    D: Default + Send + 'static,
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
            peers: peers.clone(),
            thread: Some(thread),
        }
    }
}

fn background_thread<H, D>(
    receiver: Receiver<Command>,
    sender: Sender<Event<H::Packet>>,
    peers: Peers<D>,
    mut host: H,
) where
    H: transport::Host,
    D: Default,
{
    loop {
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
                match &event {
                    &Event::Connect(peer_id) => {
                        let mut peers = peers.lock().unwrap();
                        peers.insert(peer_id, Default::default());
                    }
                    _ => {}
                }

                if let Err(_) = sender.send(event) {
                    // This should only happen while faultily shutting down -- just ignore
                    return;
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
