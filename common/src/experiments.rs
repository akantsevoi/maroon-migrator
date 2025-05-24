use std::time::Duration;

use libp2p::futures::channel::mpsc::Sender;
use log::debug;
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

    fn extract_sender_interface(&mut self) -> SenderInterface<Req, Res> {
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

    fn extract_receiver_interface(&mut self) -> ReceiverInterface<Req, Res> {
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

#[derive(Debug)]
struct SenderInterface<Req, Res> {
    pub sender: UnboundedSender<RequestWrapper<Req, Res>>,
    pub receiver: UnboundedReceiver<Res>,
}

impl<Req, Res> SenderInterface<Req, Res> {
    async fn request(&self, req: Req) -> Res {
        let (sender, receiver) = oneshot::channel::<Res>();

        _ = self.sender.send(RequestWrapper {
            request: req,
            response: sender,
        });

        receiver.await.unwrap()
    }

    fn result_awaiter<'a>(
        &'a self,
        req: Req,
    ) -> impl std::future::Future<Output = Res> + use<Req, Res> {
        let (sender, receiver) = oneshot::channel::<Res>();

        _ = self.sender.send(RequestWrapper {
            request: req,
            response: sender,
        });

        return async { receiver.await.unwrap() };
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
#[derive(Debug)]
struct TestRequest {
    id: String,
}

#[derive(Debug)]
struct TestResponse {
    id: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn experiments() {
    env_logger::init();
    let mut interface = AsyncInterface::<TestRequest, TestResponse>::new();

    let sender = interface.extract_sender_interface();
    let mut receiver = interface.extract_receiver_interface();

    // receiver imitation. That presumably will be running on some other thread
    // interface is not very nice, as you need to deal with channels
    tokio::spawn(async move {
        let mut first_request: Option<RequestWrapper<TestRequest, TestResponse>> = None;
        while let Some(wrapper) = receiver.receiver.recv().await {
            println!("got request: {}", wrapper.request.id);

            if first_request.is_none() {
                first_request = Some(wrapper);
            } else {
                _ = wrapper.response.send(TestResponse {
                    id: wrapper.request.id,
                });
                tokio::time::sleep(Duration::from_secs(1)).await;
                let prev = first_request.take().unwrap();
                _ = prev.response.send(TestResponse {
                    id: prev.request.id,
                });
            }
        }
    });

    let responder1 = sender.result_awaiter(TestRequest {
        id: "1".to_string(),
    });
    let responder2 = sender.result_awaiter(TestRequest {
        id: "2".to_string(),
    });

    tokio::spawn(async {
        let result1 = responder1.await;
        println!("result1: {:?}", result1);
    });

    tokio::spawn(async {
        let result2 = responder2.await;
        println!("result2: {:?}", result2);
    });

    tokio::time::sleep(Duration::from_secs(5)).await;
}
