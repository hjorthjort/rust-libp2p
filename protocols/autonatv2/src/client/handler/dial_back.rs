use std::{
    convert::identity,
    io,
    task::{Context, Poll},
};

use futures::{AsyncRead, AsyncWrite, AsyncWriteExt};
use futures_bounded::FuturesSet;
use libp2p_core::upgrade::{DeniedUpgrade, ReadyUpgrade};
use libp2p_swarm::{
    handler::{ConnectionEvent, FullyNegotiatedInbound, ListenUpgradeError},
    ConnectionHandler, ConnectionHandlerEvent, StreamProtocol, SubstreamProtocol,
};

use crate::{request_response::DialBack, Nonce};

use super::{DEFAULT_TIMEOUT, MAX_CONCURRENT_REQUESTS};

pub(crate) type ToBehaviour = io::Result<Nonce>;

pub struct Handler {
    inbound: FuturesSet<io::Result<Nonce>>,
}

impl Handler {
    pub(crate) fn new() -> Self {
        Self {
            inbound: FuturesSet::new(DEFAULT_TIMEOUT, MAX_CONCURRENT_REQUESTS),
        }
    }
}

impl ConnectionHandler for Handler {
    type FromBehaviour = ();
    type ToBehaviour = ToBehaviour;
    type InboundProtocol = ReadyUpgrade<StreamProtocol>;
    type OutboundProtocol = DeniedUpgrade;
    type InboundOpenInfo = ();
    type OutboundOpenInfo = ();

    fn listen_protocol(&self) -> SubstreamProtocol<Self::InboundProtocol, Self::InboundOpenInfo> {
        SubstreamProtocol::new(crate::DIAL_BACK_UPGRADE, ())
    }

    fn poll(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<
        ConnectionHandlerEvent<Self::OutboundProtocol, Self::OutboundOpenInfo, Self::ToBehaviour>,
    > {
        if let Poll::Ready(result) = self.inbound.poll_unpin(cx) {
            return Poll::Ready(ConnectionHandlerEvent::NotifyBehaviour(
                result
                    .map_err(|timeout| io::Error::new(io::ErrorKind::TimedOut, timeout))
                    .and_then(identity),
            ));
        }
        Poll::Pending
    }

    fn on_behaviour_event(&mut self, _event: Self::FromBehaviour) {}

    fn on_connection_event(
        &mut self,
        event: ConnectionEvent<
            Self::InboundProtocol,
            Self::OutboundProtocol,
            Self::InboundOpenInfo,
            Self::OutboundOpenInfo,
        >,
    ) {
        match event {
            ConnectionEvent::FullyNegotiatedInbound(FullyNegotiatedInbound {
                protocol, ..
            }) => {
                if self.inbound.try_push(perform_dial_back(protocol)).is_err() {
                    tracing::warn!("Dial back request dropped, too many requests in flight");
                }
            }
            ConnectionEvent::ListenUpgradeError(ListenUpgradeError { error, .. }) => {
                tracing::debug!("Dial back request failed: {:?}", error);
            }
            _ => {}
        }
    }

    fn connection_keep_alive(&self) -> bool {
        false
    }
}

async fn perform_dial_back(mut stream: impl AsyncRead + AsyncWrite + Unpin) -> io::Result<u64> {
    let DialBack { nonce } = DialBack::read_from(&mut stream).await?;
    stream.close().await?;
    Ok(nonce)
}