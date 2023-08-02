// Copyright 2022 Parity Technologies (UK) Ltd.
//
// Permission is hereby granted, free of charge, to any person obtaining a
// copy of this software and associated documentation files (the "Software"),
// to deal in the Software without restriction, including without limitation
// the rights to use, copy, modify, merge, publish, distribute, sublicense,
// and/or sell copies of the Software, and to permit persons to whom the
// Software is furnished to do so, subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in
// all copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS
// OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING
// FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER
// DEALINGS IN THE SOFTWARE.

use super::fingerprint::Fingerprint;
use js_sys::Reflect;
use log::{debug, trace};
use serde::Serialize;
use std::net::{IpAddr, SocketAddr};
use tinytemplate::TinyTemplate;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsValue;
use web_sys::{RtcSdpType, RtcSessionDescriptionInit};

/// Creates the SDP answer used by the client.
pub(crate) fn answer(
    addr: SocketAddr,
    server_fingerprint: &Fingerprint,
    client_ufrag: &str,
) -> RtcSessionDescriptionInit {
    let mut answer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Answer);
    answer_obj.sdp(&render_description(
        SESSION_DESCRIPTION,
        addr,
        server_fingerprint,
        client_ufrag,
    ));
    answer_obj
}

/// Creates the SDP offer.
///
/// Certificate verification is disabled which is why we hardcode a dummy fingerprint here.
pub(crate) fn offer(offer: JsValue, client_ufrag: &str) -> RtcSessionDescriptionInit {
    //JsValue to String
    let offer = Reflect::get(&offer, &JsValue::from_str("sdp")).unwrap();
    let offer = offer.as_string().unwrap();

    let lines = offer.split("\r\n");

    // find line and replace a=ice-ufrag: with "\r\na=ice-ufrag:{client_ufrag}\r\n"
    // find line andreplace a=ice-pwd: with "\r\na=ice-ufrag:{client_ufrag}\r\n"

    let mut munged_offer_sdp = String::new();

    for line in lines {
        if line.starts_with("a=ice-ufrag:") {
            munged_offer_sdp.push_str(&format!("a=ice-ufrag:{}\r\n", client_ufrag));
        } else if line.starts_with("a=ice-pwd:") {
            munged_offer_sdp.push_str(&format!("a=ice-pwd:{}\r\n", client_ufrag));
        } else if !line.is_empty() {
            munged_offer_sdp.push_str(&format!("{}\r\n", line));
        }
    }

    // remove any double \r\n
    let munged_offer_sdp = munged_offer_sdp.replace("\r\n\r\n", "\r\n");

    trace!("munged_offer_sdp: {}", munged_offer_sdp);

    // setLocalDescription
    let mut offer_obj = RtcSessionDescriptionInit::new(RtcSdpType::Offer);
    offer_obj.sdp(&munged_offer_sdp);

    offer_obj
}

// An SDP message that constitutes the offer.
//
// Main RFC: <https://datatracker.ietf.org/doc/html/rfc8866>
// `sctp-port` and `max-message-size` attrs RFC: <https://datatracker.ietf.org/doc/html/rfc8841>
// `group` and `mid` attrs RFC: <https://datatracker.ietf.org/doc/html/rfc9143>
// `ice-ufrag`, `ice-pwd` and `ice-options` attrs RFC: <https://datatracker.ietf.org/doc/html/rfc8839>
// `setup` attr RFC: <https://datatracker.ietf.org/doc/html/rfc8122>
//
// Short description:
//
// v=<protocol-version> -> always 0
// o=<username> <sess-id> <sess-version> <nettype> <addrtype> <unicast-address>
//
//     <username> identifies the creator of the SDP document. We are allowed to use dummy values
//     (`-` and `0.0.0.0` as <addrtype>) to remain anonymous, which we do. Note that "IN" means
//     "Internet".
//
// s=<session name>
//
//     We are allowed to pass a dummy `-`.
//
// c=<nettype> <addrtype> <connection-address>
//
//     Indicates the IP address of the remote.
//     Note that "IN" means "Internet".
//
// t=<start-time> <stop-time>
//
//     Start and end of the validity of the session. `0 0` means that the session never expires.
//
// m=<media> <port> <proto> <fmt> ...
//
//     A `m=` line describes a request to establish a certain protocol. The protocol in this line
//     (i.e. `TCP/DTLS/SCTP` or `UDP/DTLS/SCTP`) must always be the same as the one in the offer.
//     We know that this is true because we tweak the offer to match the protocol. The `<fmt>`
//     component must always be `webrtc-datachannel` for WebRTC.
//     RFCs: 8839, 8866, 8841
//
// a=mid:<MID>
//
//     Media ID - uniquely identifies this media stream (RFC9143).
//
// a=ice-options:ice2
//
//     Indicates that we are complying with RFC8839 (as oppposed to the legacy RFC5245).
//
// a=ice-ufrag:<ICE user>
// a=ice-pwd:<ICE password>
//
//     ICE username and password, which are used for establishing and
//     maintaining the ICE connection. (RFC8839)
//     MUST match ones used by the answerer (server).
//
// a=fingerprint:sha-256 <fingerprint>
//
//     Fingerprint of the certificate that the remote will use during the TLS
//     handshake. (RFC8122)
//
// a=setup:actpass
//
//     The endpoint that is the offerer MUST use the setup attribute value of setup:actpass and be
//     prepared to receive a client_hello before it receives the answer.
//
// a=sctp-port:<value>
//
//     The SCTP port (RFC8841)
//     Note it's different from the "m=" line port value, which indicates the port of the
//     underlying transport-layer protocol (UDP or TCP).
//
// a=max-message-size:<value>
//
//     The maximum SCTP user message size (in bytes). (RFC8841)
const CLIENT_SESSION_DESCRIPTION: &str = "v=0
o=- 0 0 IN {ip_version} {target_ip}
s=-
c=IN {ip_version} {target_ip}
t=0 0

m=application {target_port} UDP/DTLS/SCTP webrtc-datachannel
a=mid:0
a=ice-options:ice2
a=ice-ufrag:{ufrag}
a=ice-pwd:{pwd}
a=fingerprint:{fingerprint_algorithm} {fingerprint_value}
a=setup:actpass
a=sctp-port:5000
a=max-message-size:16384
";

// See [`CLIENT_SESSION_DESCRIPTION`].
//
// a=ice-lite
//
//     A lite implementation is only appropriate for devices that will *always* be connected to
//     the public Internet and have a public IP address at which it can receive packets from any
//     correspondent. ICE will not function when a lite implementation is placed behind a NAT
//     (RFC8445).
//
// a=tls-id:<id>
//
//     "TLS ID" uniquely identifies a TLS association.
//     The ICE protocol uses a "TLS ID" system to indicate whether a fresh DTLS connection
//     must be reopened in case of ICE renegotiation. Considering that ICE renegotiations
//     never happen in our use case, we can simply put a random value and not care about
//     it. Note however that the TLS ID in the answer must be present if and only if the
//     offer contains one. (RFC8842)
//     TODO: is it true that renegotiations never happen? what about a connection closing?
//     "tls-id" attribute MUST be present in the initial offer and respective answer (RFC8839).
//     XXX: but right now browsers don't send it.
//
// a=setup:passive
//
//     "passive" indicates that the remote DTLS server will only listen for incoming
//     connections. (RFC5763)
//     The answerer (server) MUST not be located behind a NAT (RFC6135).
//
//     The answerer MUST use either a setup attribute value of setup:active or setup:passive.
//     Note that if the answerer uses setup:passive, then the DTLS handshake will not begin until
//     the answerer is received, which adds additional latency. setup:active allows the answer and
//     the DTLS handshake to occur in parallel. Thus, setup:active is RECOMMENDED.
//
// a=candidate:<foundation> <component-id> <transport> <priority> <connection-address> <port> <cand-type>
//
//     A transport address for a candidate that can be used for connectivity checks (RFC8839).
//
// a=end-of-candidates
//
//     Indicate that no more candidates will ever be sent (RFC8838).
// const SERVER_SESSION_DESCRIPTION: &str = "v=0
// o=- 0 0 IN {ip_version} {target_ip}
// s=-
// t=0 0
// a=ice-lite
// m=application {target_port} UDP/DTLS/SCTP webrtc-datachannel
// c=IN {ip_version} {target_ip}
// a=mid:0
// a=ice-options:ice2
// a=ice-ufrag:{ufrag}
// a=ice-pwd:{pwd}
// a=fingerprint:{fingerprint_algorithm} {fingerprint_value}

// a=setup:passive
// a=sctp-port:5000
// a=max-message-size:16384
// a=candidate:1 1 UDP 1 {target_ip} {target_port} typ host
// a=end-of-candidates";

// Update to this:
// v=0
// o=- 0 0 IN ${ipVersion} ${host}
// s=-
// c=IN ${ipVersion} ${host}
// t=0 0
// a=ice-lite
// m=application ${port} UDP/DTLS/SCTP webrtc-datachannel
// a=mid:0
// a=setup:passive
// a=ice-ufrag:${ufrag}
// a=ice-pwd:${ufrag}
// a=fingerprint:${CERTFP}
// a=sctp-port:5000
// a=max-message-size:100000
// a=candidate:1467250027 1 UDP 1467250027 ${host} ${port} typ host\r\n
const SESSION_DESCRIPTION: &str = "v=0
o=- 0 0 IN {ip_version} {target_ip}
s=-
c=IN {ip_version} {target_ip}
t=0 0
a=ice-lite
m=application {target_port} UDP/DTLS/SCTP webrtc-datachannel
a=mid:0
a=setup:passive
a=ice-ufrag:{ufrag}
a=ice-pwd:{pwd}
a=fingerprint:{fingerprint_algorithm} {fingerprint_value}
a=sctp-port:5000
a=max-message-size:16384
a=candidate:1467250027 1 UDP 1467250027 {target_ip} {target_port} typ host
";

/// Indicates the IP version used in WebRTC: `IP4` or `IP6`.
#[derive(Serialize)]
enum IpVersion {
    IP4,
    IP6,
}

/// Context passed to the templating engine, which replaces the above placeholders (e.g.
/// `{IP_VERSION}`) with real values.
#[derive(Serialize)]
struct DescriptionContext {
    pub(crate) ip_version: IpVersion,
    pub(crate) target_ip: IpAddr,
    pub(crate) target_port: u16,
    pub(crate) fingerprint_algorithm: String,
    pub(crate) fingerprint_value: String,
    pub(crate) ufrag: String,
    pub(crate) pwd: String,
}

/// Renders a [`TinyTemplate`] description using the provided arguments.
fn render_description(
    description: &str,
    addr: SocketAddr,
    fingerprint: &Fingerprint,
    ufrag: &str,
) -> String {
    let mut tt = TinyTemplate::new();
    tt.add_template("description", description).unwrap();

    let context = DescriptionContext {
        ip_version: {
            if addr.is_ipv4() {
                IpVersion::IP4
            } else {
                IpVersion::IP6
            }
        },
        target_ip: addr.ip(),
        target_port: addr.port(),
        fingerprint_algorithm: fingerprint.algorithm(),
        fingerprint_value: fingerprint.to_sdp_format(),
        // NOTE: ufrag is equal to pwd.
        ufrag: ufrag.to_owned(),
        pwd: ufrag.to_owned(),
    };
    tt.render("description", &context).unwrap()
}

/// Parse SDP String into a JsValue
pub fn candidate(sdp: &str) -> Option<String> {
    let lines = sdp.split("\r\n");

    for line in lines {
        if line.starts_with("a=candidate:") {
            // return with leading "a=candidate:" replaced with ""
            return Some(line.replace("a=candidate:", ""));
        }
    }
    None
}

/// sdpMid
/// Get the media id from the SDP
pub fn mid(sdp: &str) -> Option<String> {
    let lines = sdp.split("\r\n");

    // lines.find(|&line| line.starts_with("a=mid:"));

    for line in lines {
        if line.starts_with("a=mid:") {
            return Some(line.replace("a=mid:", ""));
        }
    }
    None
}

/// Get Fingerprint from SDP
/// Gets the fingerprint from matching between the angle brackets: a=fingerprint:<hash-algo> <fingerprint>
pub fn fingerprint(sdp: &str) -> Result<Fingerprint, regex::Error> {
    // split the sdp by new lines / carriage returns
    let lines = sdp.split("\r\n");

    // iterate through the lines to find the one starting with a=fingerprint:
    // get the value after the first space
    // return the value as a Fingerprint
    for line in lines {
        if line.starts_with("a=fingerprint:") {
            let fingerprint = line.split(' ').nth(1).unwrap();
            let bytes = hex::decode(fingerprint.replace(':', "")).unwrap();
            let arr: [u8; 32] = bytes.as_slice().try_into().unwrap();
            return Ok(Fingerprint::raw(arr));
        }
    }
    Err(regex::Error::Syntax("fingerprint not found".to_string()))

    // let fingerprint_regex = match regex::Regex::new(
    //     r"/^a=fingerprint:(?:\w+-[0-9]+)\s(?P<fingerprint>(:?[0-9a-fA-F]{2})+)",
    // ) {
    //     Ok(fingerprint_regex) => fingerprint_regex,
    //     Err(e) => return Err(regex::Error::Syntax(format!("regex fingerprint: {}", e))),
    // };
    // let captures = match fingerprint_regex.captures(sdp) {
    //     Some(captures) => captures,
    //     None => {
    //         return Err(regex::Error::Syntax(format!(
    //             "fingerprint captures is None {}",
    //             sdp
    //         )))
    //     }
    // };
    // let fingerprint = match captures.name("fingerprint") {
    //     Some(fingerprint) => fingerprint.as_str(),
    //     None => return Err(regex::Error::Syntax("fingerprint name is None".to_string())),
    // };
    // let decoded = match hex::decode(fingerprint) {
    //     Ok(fingerprint) => fingerprint,
    //     Err(e) => {
    //         return Err(regex::Error::Syntax(format!(
    //             "decode fingerprint error: {}",
    //             e
    //         )))
    //     }
    // };
    // Ok(Fingerprint::from_certificate(&decoded))
}

/*
offer_obj: RtcSessionDescriptionInit { obj: Object { obj: JsValue(Object({"type":"offer","sdp":"v=0\r\no=- 7315842204271936257 2 IN IP4 127.0.0.1\r\ns=-\r\nt=0 0\r\na=extmap-allow-mixed\r\na=msid-semantic: WMS\r\n"})) } }
    answer_obj: RtcSessionDescriptionInit { obj: Object { obj: JsValue(Object({"type":"answer","sdp":"v=0\no=- 0 0 IN IP6 ::1\ns=-\nc=IN IP6 ::1\nt=0 0\na=ice-lite\nm=application 61885 UDP/DTLS/SCTP webrtc-datachannel\na=mid:0\na=setup:passive\na=ice-ufrag:libp2p+webrtc+v1/qBN+NUAT4icgH81g63DoyBs5x/RAQ6tE\na=ice-pwd:libp2p+webrtc+v1/qBN+NUAT4icgH81g63DoyBs5x/RAQ6tE\na=fingerprint:sha-256 A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89\na=sctp-port:5000\na=max-message-size:100000\na=candidate:1467250027 1 UDP 1467250027 ::1 61885 typ host\n"})) } }

console.log div contained:
    panicked at 'dial failed: JsError("Error setting remote_description: JsValue(InvalidAccessError: Failed to execute 'setRemoteDescription' on 'RTCPeerConnection': Failed to set remote answer sdp: The order of m-lines in answer doesn't match order in offer. Rejecting answer

// What has to change about the SDP offer in order for it to be acceptable by the given answer above:
// How m-lines work:
// M-lines mean "media lines". They are used to describe the media streams that are being negotiated.
// The m-line is the line that describes the media stream. It is composed of the following fields:
// m=<media> <port> <proto> <fmt> ...
// <media> is the type of media (audio, video, data, etc.)
// <port> is the port number that the media stream will be sent on
// <proto> is the protocol that will be used to send the media stream (RTP/SAVPF, UDP/TLS/RTP/SAVPF, etc.)
// <fmt> is the format of the media stream (VP8, H264, etc.)
// The m-line is followed by a series of attributes that describe the media stream. These attributes are called "media-level attributes" and are prefixed with an "a=".
// The order of the m-lines in the answer must match the order of the m-lines in the offer.
// The order of the media-level attributes in the answer must match the order of the media-level attributes in the offer.
// For example, if the offer has the following data channel m-lines:
// m=application 9 UDP/DTLS/SCTP webrtc-datachannel
// a=sctp-port:5000
// a=max-message-size:16384
// a=candidate:1 1 UDP 1
// The answer must have the following data channel m-lines:
// m=application 9 UDP/DTLS/SCTP webrtc-datachannel
// a=sctp-port:5000
// a=max-message-size:16384
// a=candidate:1 1 UDP 1
// When the browser API creates the offer, it will always put the data channel m-line first. This means that the answer must also have the data channel m-line first.

The differences between a STUN message and the SDP are:
STUN messages are sent over UDP, while SDP messages are sent over TCP.
STUN messages are used to establish a connection, while SDP messages are used to describe the connection.
STUN message looks like:
*/

// run test for any, none or all features
#[cfg(test)]
mod sdp_tests {
    use super::*;

    #[test]
    fn test_fingerprint() -> Result<(), regex::Error> {
        let val = b"A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89";
        let sdp: &str = "v=0\no=- 0 0 IN IP6 ::1\ns=-\nc=IN IP6 ::1\nt=0 0\na=ice-lite\nm=application 61885 UDP/DTLS/SCTP webrtc-datachannel\na=mid:0\na=setup:passive\na=ice-ufrag:libp2p+webrtc+v1/YwapWySn6fE6L9i47PhlB6X4gzNXcgFs\na=ice-pwd:libp2p+webrtc+v1/YwapWySn6fE6L9i47PhlB6X4gzNXcgFs\na=fingerprint:sha-256 A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89\na=sctp-port:5000\na=max-message-size:16384\na=candidate:1467250027 1 UDP 1467250027 ::1 61885 typ host\n";
        let fingerprint = fingerprint(sdp)?;
        assert_eq!(fingerprint.algorithm(), "sha-256");
        assert_eq!(fingerprint.to_sdp_format(), "A8:17:77:1E:02:7E:D1:2B:53:92:70:A6:8E:F9:02:CC:21:72:3A:92:5D:F4:97:5F:27:C4:5E:75:D4:F4:31:89");
        Ok(())
    }
}