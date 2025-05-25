use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};
use tokio::sync::oneshot;

pub fn create_invoker_handler_pair<Req, Res>()
-> (InvokerInterface<Req, Res>, HandlerInterface<Req, Res>) {
    let (tx, rx) = mpsc::unbounded_channel::<RequestWrapper<Req, Res>>();
    (
        InvokerInterface { sender: tx },
        HandlerInterface { receiver: rx },
    )
}

#[derive(Debug)]
pub struct InvokerInterface<Req, Res> {
    sender: UnboundedSender<RequestWrapper<Req, Res>>,
}

impl<Req, Res> InvokerInterface<Req, Res> {
    // pub async fn request(&self, req: Req) -> Res {
    //     let (sender, receiver) = oneshot::channel::<Res>();

    //     _ = self.sender.send(RequestWrapper {
    //         request: req,
    //         response: sender,
    //     });

    //     receiver.await.unwrap()
    // }

    // pub fn request(&self, req: Req) -> impl std::future::Future<Output = Res> + use<Req, Res> {
    //     let (sender, receiver) = oneshot::channel::<Res>();

    //     _ = self.sender.send(RequestWrapper {
    //         request: req,
    //         response: sender,
    //     });

    //     return async { receiver.await.unwrap() };
    // }

    pub fn request(&self, req: Req) -> ResultFuture<Res> {
        let (sender, receiver) = oneshot::channel::<Res>();

        if let Err(err) = self.sender.send(RequestWrapper {
            request: req,
            response: sender,
        }) {
            todo!("{err}")
        };

        return ResultFuture { receiver: receiver };
    }
}

pub struct ResultFuture<Res> {
    receiver: oneshot::Receiver<Res>,
}

impl<Res> Future for ResultFuture<Res> {
    type Output = Res;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let oneshot_pin = Pin::new(&mut self.receiver);

        match oneshot_pin.poll(cx) {
            Poll::Ready(Ok(res)) => Poll::Ready(res),
            Poll::Ready(Err(_canceled)) => todo!("sender was dropped unexpectedly"),
            Poll::Pending => Poll::Pending,
        }
    }
}

pub struct HandlerInterface<Req, Res> {
    /// TODO: is there a way to hide the channel and make handler a bit more generic?
    /// right now I don't hide it because there is no nice interface I know about to use instead of while .recv() or tokio::select
    pub receiver: UnboundedReceiver<RequestWrapper<Req, Res>>,
}

pub struct RequestWrapper<Req, Res> {
    pub request: Req,
    pub response: oneshot::Sender<Res>,
}

/// END of library code

#[cfg(test)]
#[derive(Debug)]
struct TestRequest {
    id: String,
}

#[cfg(test)]
#[derive(Debug)]
struct TestResponse {
    id: String,
}

#[tokio::test(flavor = "multi_thread")]
async fn test_invoker_handler() {
    use std::time::Duration;
    env_logger::init();

    let (invoker, mut handler) = create_invoker_handler_pair::<TestRequest, TestResponse>();

    // handler imitation. That presumably will be running on some other thread
    // interface for the handler is not as nice as it is for invoker(space for improvement)
    //
    // here I'm imitating that responses might be sent in a different order compare to requests sent
    tokio::spawn(async move {
        let mut first_request: Option<RequestWrapper<TestRequest, TestResponse>> = None;
        while let Some(wrapper) = handler.receiver.recv().await {
            println!("got request: {}", wrapper.request.id);

            if first_request.is_none() {
                first_request = Some(wrapper);
            } else {
                _ = wrapper.response.send(TestResponse {
                    id: wrapper.request.id,
                });
                tokio::time::sleep(Duration::from_millis(100)).await;
                let prev = first_request.take().unwrap();
                _ = prev.response.send(TestResponse {
                    id: prev.request.id,
                });
            }
        }
    });

    let responder1 = invoker.request(TestRequest {
        id: "1".to_string(),
    });
    let responder2 = invoker.request(TestRequest {
        id: "2".to_string(),
    });

    let task2 = tokio::spawn(async {
        let result2 = responder2.await;
        println!("result2: {:?}", result2);
        return result2;
    });

    let result1 = responder1.await;
    println!("result1: {:?}", result1);

    assert_eq!("2".to_string(), task2.await.unwrap().id);
    assert_eq!("1".to_string(), result1.id);
}
