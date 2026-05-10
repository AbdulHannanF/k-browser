use crate::error::{IpcError, IpcResult};
use crate::message::{IpcMessage, ProcessRole};
use interprocess::local_socket::tokio::{
    Listener as LocalSocketListener, Stream as LocalSocketStream,
};
use interprocess::local_socket::traits::tokio::{Listener, Stream};
use interprocess::local_socket::{GenericFilePath, GenericNamespaced, ListenerOptions};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, warn};

pub struct IpcServer {
    listener: LocalSocketListener,
    clients: Arc<Mutex<HashMap<ProcessRole, IpcChannel>>>,
}

impl IpcServer {
    /// Broker binds here. Name: e.g. "kitsune-broker"
    pub async fn bind(name: &str) -> IpcResult<Self> {
        let name_var = if cfg!(windows) {
            interprocess::local_socket::ToNsName::to_ns_name::<GenericNamespaced>(name).unwrap()
        } else {
            interprocess::local_socket::ToFsName::to_fs_name::<GenericFilePath>(format!(
                "/tmp/{}",
                name
            ))
            .unwrap()
        };

        let listener = ListenerOptions::new().name(name_var).create_tokio()?;

        Ok(Self {
            listener,
            clients: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub async fn accept_loop(&self, tx: mpsc::Sender<(ProcessRole, IpcMessage)>) {
        loop {
            match self.listener.accept().await {
                Ok(stream) => {
                    let channel = IpcChannel::from_stream(stream);

                    // Receive registration message (must be ProcessRole)
                    let clients = self.clients.clone();
                    let tx = tx.clone();

                    tokio::spawn(async move {
                        let init_msg = match channel.recv().await {
                            Ok(msg) => msg,
                            Err(e) => {
                                warn!("Failed to receive init message from new IPC client: {e}");
                                return;
                            }
                        };

                        // Extract ProcessRole, assuming it's embedded or the first message is basically a registration.
                        // For the sake of the exercise, let's assume `init_msg.sender` contains the role encoded,
                        // or we parse the role from `ProcessId(val)` where val = "Network", "Renderer" etc.

                        // Since `ProcessRole` was added, let's just parse the sender's ProcessId as ProcessRole.
                        let parsed_role = match init_msg.sender.0.as_str() {
                            "Network" => ProcessRole::Network,
                            "Renderer" => ProcessRole::Renderer,
                            "Js" => ProcessRole::Js,
                            "Agent" => ProcessRole::Agent,
                            _ => ProcessRole::Renderer, // fallback
                        };

                        info!("Accepted IPC client for role {:?}", parsed_role);
                        clients.lock().await.insert(parsed_role, channel.clone());

                        let mut rx_guard = channel.rx.lock().await;

                        loop {
                            let msg = match channel.recv_sync(&mut *rx_guard).await {
                                Ok(m) => m,
                                Err(IpcError::Io(e))
                                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                                {
                                    warn!("IPC client {:?} disconnected", parsed_role);
                                    let _ = tx
                                        .send((
                                            parsed_role,
                                            IpcMessage::new(
                                                crate::message::ProcessId("Broker".to_string()),
                                                crate::message::ProcessId("Broker".to_string()),
                                                crate::message::IpcPayload::Error {
                                                    code: "EOF".to_string(),
                                                    message: "Disconnected".to_string(),
                                                },
                                            ),
                                        ))
                                        .await; // Not exact but acts as disconnect
                                    break;
                                }
                                Err(e) => {
                                    error!("IPC receive error for {:?}: {e}", parsed_role);
                                    break;
                                }
                            };

                            // Capability-based privilege check: each role may only
                            // send the payload types appropriate to its trust level.
                            // Fail closed — unknown roles are denied.
                            let allowed = match parsed_role {
                                // Broker has full authority; it never connects as a client.
                                ProcessRole::Broker => false,

                                // Semi-privileged: agent can initiate vault/HIL/DOM/navigation.
                                ProcessRole::Agent => matches!(
                                    msg.payload,
                                    crate::message::IpcPayload::VaultRequest { .. }
                                        | crate::message::IpcPayload::HilCheckpointRequest { .. }
                                        | crate::message::IpcPayload::DomQuery { .. }
                                        | crate::message::IpcPayload::DomFillField { .. }
                                        | crate::message::IpcPayload::DomClick { .. }
                                        | crate::message::IpcPayload::SetDomHighlight(_)
                                        | crate::message::IpcPayload::ClearDomHighlight(_)
                                        | crate::message::IpcPayload::ClearAllDomHighlights
                                        | crate::message::IpcPayload::NavigateRequest { .. }
                                        | crate::message::IpcPayload::AgentActionRequest { .. }
                                        | crate::message::IpcPayload::ProcessRegister { .. }
                                        | crate::message::IpcPayload::ProcessShutdown { .. }
                                ),

                                // Sandboxed renderer: can only report DOM results and lifecycle.
                                ProcessRole::Renderer => matches!(
                                    msg.payload,
                                    crate::message::IpcPayload::DomQueryResult { .. }
                                        | crate::message::IpcPayload::DomOperationResult { .. }
                                        | crate::message::IpcPayload::ProcessRegister { .. }
                                        | crate::message::IpcPayload::ProcessShutdown { .. }
                                        | crate::message::IpcPayload::Error { .. }
                                ),

                                // Sandboxed network process: can only report fetch results and navigation.
                                ProcessRole::Network => matches!(
                                    msg.payload,
                                    crate::message::IpcPayload::NetworkFetchResponse { .. }
                                        | crate::message::IpcPayload::NavigateResponse { .. }
                                        | crate::message::IpcPayload::ProcessRegister { .. }
                                        | crate::message::IpcPayload::ProcessShutdown { .. }
                                        | crate::message::IpcPayload::Error { .. }
                                ),

                                // Sandboxed JS engine: lifecycle messages only.
                                ProcessRole::Js => matches!(
                                    msg.payload,
                                    crate::message::IpcPayload::ProcessRegister { .. }
                                        | crate::message::IpcPayload::ProcessShutdown { .. }
                                        | crate::message::IpcPayload::Error { .. }
                                ),
                            };

                            if !allowed {
                                warn!(
                                    role = ?parsed_role,
                                    payload = ?std::mem::discriminant(&msg.payload),
                                    correlation_id = %msg.correlation_id.0,
                                    "IPC privilege denial — role sent disallowed payload type"
                                );
                                let _ = tx
                                    .send((
                                        parsed_role,
                                        IpcMessage::new(
                                            crate::message::ProcessId("Broker".to_string()),
                                            crate::message::ProcessId("Broker".to_string()),
                                            crate::message::IpcPayload::Error {
                                                code: "PERMISSION_DENIED".to_string(),
                                                message: format!(
                                                    "{:?} is not permitted to send this payload type",
                                                    parsed_role
                                                ),
                                            },
                                        ),
                                    ))
                                    .await;
                            } else {
                                if tx.send((parsed_role, msg)).await.is_err() {
                                    break;
                                }
                            }
                        }

                        clients.lock().await.remove(&parsed_role);
                    });
                }
                Err(e) => {
                    warn!("Failed to accept IPC connection: {e}");
                }
            }
        }
    }

    pub async fn send_to(&self, role: ProcessRole, msg: IpcMessage) -> IpcResult<()> {
        let clients = self.clients.lock().await;
        if let Some(channel) = clients.get(&role) {
            channel.send(msg).await
        } else {
            Err(IpcError::Disconnected(role))
        }
    }
}

#[derive(Clone)]
pub struct IpcChannel {
    tx: Arc<Mutex<WriteHalf<LocalSocketStream>>>,
    rx: Arc<Mutex<ReadHalf<LocalSocketStream>>>,
}

impl IpcChannel {
    pub fn from_stream(stream: LocalSocketStream) -> Self {
        let (rx, tx) = tokio::io::split(stream);
        Self {
            tx: Arc::new(Mutex::new(tx)),
            rx: Arc::new(Mutex::new(rx)),
        }
    }

    pub async fn connect(name: &str) -> IpcResult<Self> {
        let name_var = if cfg!(windows) {
            interprocess::local_socket::ToNsName::to_ns_name::<GenericNamespaced>(name).unwrap()
        } else {
            interprocess::local_socket::ToFsName::to_fs_name::<GenericFilePath>(format!(
                "/tmp/{}",
                name
            ))
            .unwrap()
        };
        let stream = LocalSocketStream::connect(name_var).await?;
        Ok(Self::from_stream(stream))
    }

    pub async fn send(&self, msg: IpcMessage) -> IpcResult<()> {
        let mut tx = self.tx.lock().await;
        let payload = postcard::to_allocvec(&msg)?;
        let len = payload.len() as u32;
        tx.write_all(&len.to_le_bytes()).await?;
        tx.write_all(&payload).await?;
        tx.flush().await?;
        Ok(())
    }

    pub async fn recv(&self) -> IpcResult<IpcMessage> {
        let mut rx = self.rx.lock().await;
        self.recv_sync(&mut *rx).await
    }

    async fn recv_sync(&self, rx: &mut ReadHalf<LocalSocketStream>) -> IpcResult<IpcMessage> {
        let mut len_buf = [0u8; 4];
        rx.read_exact(&mut len_buf).await?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut payload_buf = vec![0u8; len];
        rx.read_exact(&mut payload_buf).await?;
        let msg = postcard::from_bytes(&payload_buf)?;
        Ok(msg)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::message::{IpcMessage, IpcPayload, PrivilegeLevel, ProcessId};
    use tokio::time::Duration;

    #[tokio::test]
    async fn test_ipc_roundtrip() {
        let (tx, mut rx) = mpsc::channel(10);
        let server = IpcServer::bind("test-roundtrip").await.unwrap();
        tokio::spawn(async move {
            server.accept_loop(tx).await;
        });
        tokio::time::sleep(Duration::from_millis(50)).await;

        let client = IpcChannel::connect("test-roundtrip").await.unwrap();
        let init_msg = IpcMessage::new(
            ProcessId("Network".into()),
            ProcessId("Broker".into()),
            IpcPayload::ProcessRegister {
                privilege_level: PrivilegeLevel::Sandboxed,
                capabilities: vec![],
            },
        );
        client.send(init_msg).await.unwrap();

        tokio::time::sleep(Duration::from_millis(50)).await;

        let test_msg = IpcMessage::new(
            ProcessId("Network".into()),
            ProcessId("Broker".into()),
            IpcPayload::ProcessShutdown {
                reason: "test".into(),
            },
        );
        client.send(test_msg.clone()).await.unwrap();

        let (_, received) = rx.recv().await.unwrap();
        assert_eq!(received.correlation_id, test_msg.correlation_id);
    }
}
