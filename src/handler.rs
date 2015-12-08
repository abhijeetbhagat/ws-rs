use url;
use log::LogLevel::Error as ErrorLevel;
use openssl::ssl::{Ssl, SslContext, SslMethod, IntoSsl};

use message::Message;
use frame::Frame;
use protocol::CloseCode;
use handshake::{Handshake, Request, Response};
use result::{Result, Error, Kind};


/// The core trait of this library.
/// Implementing this trait provides the business logic of the WebSocket application.
pub trait Handler {

    // general

    /// Called when a request to shutdown all connections has been received.
    #[inline]
    fn on_shutdown(&mut self) {
        debug!("Handler received WebSocket shutdown request.");
    }

    // WebSocket events

    /// Called when the WebSocket handshake is successful and the connection is open for sending
    /// and receiving messages.
    fn on_open(&mut self, shake: Handshake) -> Result<()> {
        debug!("Connection opened with {:?}", shake);
        Ok(())
    }

    /// Called on incoming messages.
    fn on_message(&mut self, msg: Message) -> Result<()> {
        debug!("Received message {:?}", msg);
        Ok(())
    }

    /// Called when the other endpoint is asking to close the connection.
    fn on_close(&mut self, code: CloseCode, reason: &str) {
        debug!("Connection closing due to ({:?}) {}", code, reason);
    }

    /// Called when an error occurs on the WebSocket.
    fn on_error(&mut self, err: Error) {
        // Ignore connection reset errors by default, but allow library clients to see them by
        // overriding this method if they want
        if let Kind::Io(ref err) = err.kind {
            if let Some(104) = err.raw_os_error() {
                return
            }
        }

        error!("{:?}", err);
        if !log_enabled!(ErrorLevel) {
            println!("Encountered an error: {}\nEnable a logger to see more information.", err);
        }
    }

    // handshake events

    /// A method for handling the low-level workings of the request portion of the WebSocket
    /// handshake.
    ///
    /// Implementors should select a WebSocket protocol and extensions where they are supported.
    ///
    /// Implementors can inspect the Request and must return a Response or an error
    /// indicating that the handshake failed. The default implementation provides conformance with
    /// the WebSocket protocol, and implementors should use the `Response::from_request` method and
    /// then modify the resulting response as necessary in order to maintain conformance.
    ///
    /// This method will not be called when the handler represents a client endpoint. Use
    /// `build_request` to provide an initial handshake request.
    ///
    /// # Examples
    ///
    /// ```ignore
    /// let mut res = try!(Response::from_request(req));
    /// if req.extensions.contains("myextension") {
    ///     res.add_extension("myextension")
    /// }
    /// Ok(res)
    ///
    #[inline]
    fn on_request(&mut self, req: &Request) -> Result<Response> {
        debug!("Handler received request:\n{}", req);
        Response::from_request(req)
    }

    /// A method for handling the low-level workings of the response portion of the WebSocket
    /// handshake.
    ///
    /// Implementors can inspect the Response and choose to fail the connection by
    /// returning an error. This method will not be called when the handler represents a server
    /// endpoint. The response should indicate which WebSocket protocol and extensions the server
    /// has agreed to if any.
    #[inline]
    fn on_response(&mut self, res: &Response) -> Result<()> {
        debug!("Handler received response:\n{}", res);
        Ok(())
    }

    // frame events

    /// A method for handling incoming frames.
    ///
    /// This method provides very low-level access to the details of the WebSocket protocol. It may
    /// be necessary to implement this method in order to provide a particular extension, but
    /// incorrect implementation may cause the other endpoint to fail the connection.
    ///
    /// Returning `Ok(None)` will cause the connection to forget about a particular frame. This is
    /// useful if you want ot filter out a frame or if you don't want any of the default handler
    /// methods to run.
    ///
    /// By default this method simply ensures that no reserved bits are set.
    #[inline]
    fn on_frame(&mut self, frame: Frame) -> Result<Option<Frame>> {
        debug!("Handler received: {}", frame);
        // default implementation doesn't allow for reserved bits to be set
        if frame.has_rsv1() || frame.has_rsv2() || frame.has_rsv3() {
            Err(Error::new(Kind::Protocol, "Encountered frame with reserved bits set."))
        } else {
            Ok(Some(frame))
        }
    }

    /// A method for handling outgoing frames.
    ///
    /// This method provides very low-level access to the details of the WebSocket protocol. It may
    /// be necessary to implement this method in order to provide a particular extension, but
    /// incorrect implementation may cause the other endpoint to fail the connection.
    ///
    /// Returning `Ok(None)` will cause the connection to forget about a particular frame, meaning
    /// that it will not be sent. You can use this approach to merge multiple frames into a single
    /// frame before sending the message.
    ///
    /// By default this method simply ensures that no reserved bits are set.
    #[inline]
    fn on_send_frame(&mut self, frame: Frame) -> Result<Option<Frame>> {
        debug!("Handler will send: {}", frame);
        // default implementation doesn't allow for reserved bits to be set
        if frame.has_rsv1() || frame.has_rsv2() || frame.has_rsv3() {
            Err(Error::new(Kind::Protocol, "Encountered frame with reserved bits set."))
        } else {
            Ok(Some(frame))
        }
    }

    // constructors

    /// A method for creating the initial handshake request for WebSocket clients.
    ///
    /// The default implementation provides conformance with the WebSocket protocol, but this
    /// method may be overriden. In order to facilitate conformance,
    /// implementors should use the `Request::from_url` method and then modify the resulting
    /// request as necessary.
    ///
    /// Implementors should indicate any available WebSocket extensions here.
    ///
    /// # Examples
    /// ```ignore
    /// let mut req = try!(Request::from_url(url));
    /// req.add_extension("permessage-deflate; client_max_window_bits");
    /// Ok(req)
    /// ```
    #[inline]
    fn build_request(&mut self, url: &url::Url) -> Result<Request> {
        debug!("Handler is building request from {}.", url);
        Request::from_url(url)
    }

    /// A method for obtaining an Ssl object for use in wss connections.
    ///
    /// Override this method to customize the Ssl object used to encrypt the connection.
    #[inline]
    fn build_ssl(&mut self) -> Result<Ssl> {
        let context = try!(SslContext::new(SslMethod::Tlsv1));
        (&context).into_ssl().map_err(Error::from)
    }
}

impl<F> Handler for F
    where F: Fn(Message) -> Result<()>
{
    fn on_message(&mut self, msg: Message) -> Result<()> {
        self(msg)
    }
}

mod test {
    #![allow(unused_imports, unused_variables, dead_code)]
    use super::*;
    use url;
    use mio;
    use handshake::{Request, Response, Handshake};
    use protocol::CloseCode;
    use frame;
    use message;
    use result::Result;

    #[derive(Debug, Eq, PartialEq)]
    struct M;
    impl Handler for M {
        fn on_message(&mut self, _: message::Message) -> Result<()> {
            Ok(println!("test"))
        }

        fn on_frame(&mut self, f: frame::Frame) -> Result<Option<frame::Frame>> {
            Ok(None)
        }
    }

    #[test]
    fn test_handler() {
        struct H;

        impl Handler for H {

            fn on_open(&mut self, shake: Handshake) -> Result<()> {
                assert!(shake.request.key().is_ok());
                assert!(shake.response.key().is_ok());
                Ok(())
            }

            fn on_message(&mut self, msg: message::Message) -> Result<()> {
                Ok(assert_eq!(msg, message::Message::Text(String::from("testme"))))
            }

            fn on_close(&mut self, code: CloseCode, _: &str) {
                assert_eq!(code, CloseCode::Normal)
            }

        }

        let mut h = H;
        let url = url::Url::parse("wss://127.0.0.1:3012").unwrap();
        let req = Request::from_url(&url).unwrap();
        let res = Response::from_request(&req).unwrap();
        h.on_open(Handshake{
            request: req,
            response: res,
            peer_addr: None,
            local_addr: None,
        }).unwrap();
        h.on_message(message::Message::Text("testme".to_owned())).unwrap();
        h.on_close(CloseCode::Normal, "");
    }

    #[test]
    fn test_closure_handler() {
        let mut close = |msg| {
            assert_eq!(msg, message::Message::Binary(vec![1, 2, 3]));
            Ok(())
        };

        close.on_message(message::Message::Binary(vec![1, 2, 3])).unwrap();
    }
}

