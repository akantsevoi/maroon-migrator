use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

/// Start of library code
struct AsyncInterface<Req, Res> {
    tx_request: Option<UnboundedSender<RequestWrapper<Req, Res>>>,
    rx_request: Option<UnboundedReceiver<RequestWrapper<Req, Res>>>,
    tx_response: Option<UnboundedSender<Res>>,
    rx_response: Option<UnboundedReceiver<Res>>,
}

impl<Req, Res> AsyncInterface<Req, Res> {
    fn new() -> AsyncInterface<Req, Res> {
        let (tx_request, rx_request) = mpsc::unbounded_channel::<RequestWrapper<Req, Res>>();
        let (tx_response, rx_response) = mpsc::unbounded_channel::<Res>();

        AsyncInterface {
            tx_request: Some(tx_request),
            rx_request: Some(rx_request),
            tx_response: Some(tx_response),
            rx_response: Some(rx_response),
        }
    }

    fn sender_interface(&mut self) -> SenderInterface<Req, Res> {
        SenderInterface {
            sender: self
                .tx_request
                .take()
                .expect("should be moved out only once"),
            receiver: self
                .rx_response
                .take()
                .expect("should be moved out only once"),
        }
    }

    fn receiver_interface(&mut self) -> ReceiverInterface<Req, Res> {
        ReceiverInterface {
            sender: self
                .tx_response
                .take()
                .expect("should be moved out only once"),
            receiver: self
                .rx_request
                .take()
                .expect("should be moved out only once"),
        }
    }
}

struct SenderInterface<Req, Res> {
    pub sender: UnboundedSender<RequestWrapper<Req, Res>>,
    pub receiver: UnboundedReceiver<Res>,
}

impl<Req, Res> SenderInterface<Req, Res> {
    async fn request(self, req: Req) -> Res {
        let (sender, receiver) = oneshot::channel::<Res>();

        _ = self.sender.send(RequestWrapper {
            request: req,
            response: sender,
        });

        let result = receiver.await;
        return result.unwrap();
    }
}

struct ReceiverInterface<Req, Res> {
    pub sender: UnboundedSender<Res>,
    pub receiver: UnboundedReceiver<RequestWrapper<Req, Res>>,
}

struct Interface {}

struct RequestWrapper<Req, Res> {
    request: Req,
    response: oneshot::Sender<Res>,
}

/// END of library code

struct TestRequest {
    id: String,
}

struct TestResponse {
    id: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn experiments() {
    let mut interface = AsyncInterface::<TestRequest, TestResponse>::new();

    let sender = interface.sender_interface();
    let mut receiver = interface.receiver_interface();

    // receiver imitation. That presumably will be running on some other thread
    // interface is not very nice, as you need to deal with channels
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(wrapper) = receiver.receiver.recv() => {
                    // TODO: do smth with request, bla bla
                    _ = wrapper.response.send(TestResponse {
                        id: wrapper.request.id,
                    });
                },
            }
        }
    });

    // Nice and elegant interface from the sender side
    let response42 = sender
        .request(TestRequest {
            id: String::from("42"),
        })
        .await;

    assert_eq!(String::from("42"), response42.id);
}
