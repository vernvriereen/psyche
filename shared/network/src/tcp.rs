use anyhow::{anyhow, bail, Result};
use futures_util::{future::join_all, SinkExt, StreamExt};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Debug, marker::PhantomData, net::SocketAddr, sync::Arc};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tracing::{error, info};
use psyche_core::{Networkable, NodeIdentity};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::{mpsc, Mutex},
};

const MAX_FRAME_LENGTH: usize = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize, Debug)]
enum ServerToClientMessage<T: Debug> {
    Challenge([u8; 32]),
    Else(T),
}

#[derive(Serialize, Deserialize, Debug)]
enum ClientToServerMessage<T: Debug> {
    ChallengeResponse(Vec<u8>),
    Else(T),
}

pub struct TcpServer<I, ToServerMessage, ToClientMessage>
where
    I: NodeIdentity,
    ToServerMessage: Networkable + Debug + Send + Sync + 'static,
    ToClientMessage: Networkable + Debug + Send + Sync + 'static,
{
    clients: Arc<Mutex<HashMap<I, mpsc::Sender<ToClientMessage>>>>,
    _phantom: PhantomData<ToServerMessage>,

    incoming_msg_stream: tokio_stream::wrappers::ReceiverStream<(I, ToServerMessage)>,
    send_msg: mpsc::Sender<(I, ToClientMessage)>,
    local_addr: SocketAddr,
}

impl<I, ToServer, ToClient> TcpServer<I, ToServer, ToClient>
where
    I: NodeIdentity,
    ToServer: Networkable + Clone + Debug + Send + Sync + 'static,
    ToClient: Networkable + Clone + Debug + Send + Sync + 'static,
{
    pub async fn start(addr: SocketAddr) -> Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        info!("Server listening on: {}", local_addr);

        let (incoming_tx, incoming_rx) = mpsc::channel(100);
        let (send_msg, mut outgoing_rx) = mpsc::channel(100);

        let clients = Arc::new(Mutex::new(HashMap::new()));

        tokio::spawn({
            let clients = clients.clone();
            async move {
                while let Ok((stream, _)) = listener.accept().await {
                    let clients = clients.clone();
                    let incoming_tx = incoming_tx.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_connection(stream, clients, incoming_tx).await
                        {
                            error!("Error handling connection: {:?}", e);
                        }
                    });
                }
            }
        });

        tokio::spawn({
            let clients = clients.clone();
            async move {
                while let Some((id, message)) = outgoing_rx.recv().await {
                    if let Some(client) = clients.lock().await.get(&id) {
                        if let Err(e) = client.send(message).await {
                            error!("Failed to send message to client {:?}: {:?}", id, e);
                        }
                    }
                }
            }
        });

        Ok(Self {
            _phantom: Default::default(),
            clients,
            incoming_msg_stream: tokio_stream::wrappers::ReceiverStream::new(incoming_rx),
            send_msg,
            local_addr,
        })
    }

    pub fn local_addr(&self) -> &SocketAddr {
        &self.local_addr
    }

    async fn handle_connection(
        stream: TcpStream,
        clients: Arc<Mutex<HashMap<I, mpsc::Sender<ToClient>>>>,
        incoming_tx: mpsc::Sender<(I, ToServer)>,
    ) -> Result<()> {
        let mut framed = Framed::new(stream, LengthDelimitedCodec::new());

        // Generate and send challenge
        let mut challenge = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut challenge);
        framed
            .send(
                ServerToClientMessage::<ToClient>::Challenge(challenge)
                    .to_bytes()
                    .into(),
            )
            .await?;
        info!("new client joined - sent challenge {:?}", challenge);

        // Receive and verify challenge response
        let response = ClientToServerMessage::<ToClient>::from_bytes(
            &framed
                .next()
                .await
                .ok_or_else(|| anyhow!("No response received"))??,
        )?;
        let challenge_response = if let ClientToServerMessage::ChallengeResponse(res) = response {
            res
        } else {
            bail!(
                "Invalid client-to-server message - expected ChallengeResponse, got {:?}",
                response
            );
        };
        info!("got response for challenge {:?}", challenge);
        let identity = I::from_signed_bytes(&challenge_response, challenge)?;
        info!("challenge response accepted! welcome, {:?}!", identity);
        let (client_tx, mut client_rx) = mpsc::channel(32);
        clients.lock().await.insert(identity.clone(), client_tx);

        loop {
            tokio::select! {
                Some(message) = client_rx.recv() => {
                    framed.send(ServerToClientMessage::Else(message).to_bytes().into()).await?;
                }
                result = framed.next() => match result {
                    Some(Ok(bytes)) => {
                        let message = ClientToServerMessage::<ToServer>::from_bytes(&bytes)?;
                        match message {
                            ClientToServerMessage::ChallengeResponse(..) => {
                               bail!("Unexpected challenge message");
                            }
                            ClientToServerMessage::Else(m) => {
                                incoming_tx.send((identity.clone(), m)).await?;
                            }
                        }
                    }
                    Some(Err(e)) => {
                        error!("Error reading from stream: {:?}", e);
                        break;
                    }
                    None => break,
                },
            }
        }

        clients.lock().await.remove(&identity);
        Ok(())
    }

    pub async fn get_connected_clients(&self) -> Vec<I> {
        self.clients
            .lock()
            .await
            .iter()
            .map(|(identity, _)| identity.clone())
            .collect()
    }

    pub async fn next(&mut self) -> Option<(I, ToServer)> {
        self.incoming_msg_stream.next().await
    }

    pub async fn send_to(&mut self, to: I, msg: ToClient) -> Result<()> {
        self.send_msg.send((to, msg)).await.map_err(|e| e.into())
    }

    pub async fn broadcast(&mut self, msg: ToClient) -> Result<()> {
        let clients = self.get_connected_clients().await;
        let mut v = vec![];
        for to in clients {
            v.push(self.send_msg.send((to, msg.clone())));
        }
        join_all(v)
            .await
            .into_iter()
            .map(|v| v.map_err(|e| e.into()))
            .collect::<Result<Vec<_>>>()?;
        Ok(())
    }
}

pub struct TcpClient<I, ToServerMessage, ToClientMessage>
where
    I: NodeIdentity,
    ToServerMessage: Networkable + Debug + Send + Sync + 'static,
    ToClientMessage: Networkable + Debug + Send + Sync + 'static,
{
    identity: I,
    framed: Framed<TcpStream, LengthDelimitedCodec>,
    _phantom: PhantomData<(ToServerMessage, ToClientMessage)>,
}

impl<I, ToServer, ToClient> TcpClient<I, ToServer, ToClient>
where
    I: NodeIdentity,
    ToServer: Networkable + Debug + Send + Sync + 'static,
    ToClient: Networkable + Debug + Send + Sync + 'static,
{
    pub async fn connect(addr: &str, identity: I, private_key: I::PrivateKey) -> Result<Self> {
        let stream = TcpStream::connect(addr).await?;
        info!("Connected to server at: {}", addr);

        let mut codec = LengthDelimitedCodec::new();
        codec.set_max_frame_length(MAX_FRAME_LENGTH);
        let mut framed = Framed::new(stream, codec);

        // Receive challenge
        let challenge = match Self::receive_message(&mut framed).await? {
            ServerToClientMessage::Challenge(c) => c,
            _ => return Err(anyhow!("Expected challenge, got something else")),
        };

        // Sign and send challenge response
        let response = identity.to_signed_bytes(&private_key, challenge);
        framed
            .send(
                ClientToServerMessage::<ToServer>::ChallengeResponse(response)
                    .to_bytes()
                    .into(),
            )
            .await?;

        Ok(Self {
            identity,
            framed,
            _phantom: Default::default(),
        })
    }

    async fn receive_message(
        framed: &mut Framed<TcpStream, LengthDelimitedCodec>,
    ) -> Result<ServerToClientMessage<ToClient>> {
        let bytes = framed
            .next()
            .await
            .ok_or_else(|| anyhow!("Connection closed"))??;
        ServerToClientMessage::from_bytes(&bytes)
    }

    pub async fn send(&mut self, message: ToServer) -> Result<()> {
        self.framed
            .send(ClientToServerMessage::Else(message).to_bytes().into())
            .await
            .map_err(|e| e.into())
    }

    pub async fn receive(&mut self) -> Result<ToClient> {
        match Self::receive_message(&mut self.framed).await? {
            ServerToClientMessage::Else(message) => Ok(message),
            ServerToClientMessage::Challenge(_) => Err(anyhow!("Unexpected challenge message")),
        }
    }

    pub fn get_identity(&self) -> &I {
        &self.identity
    }
}
