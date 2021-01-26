use crate::dispatching::{
    core::{HandleFuture, Handler, IntoHandler},
    dispatcher_context::DispatcherContext,
};
use std::{marker::PhantomData, sync::Arc};

pub struct ParserHandler<ParserT, Upd, NextUpd, Rest, Err, HandlerT> {
    parser: Arc<ParserT>,
    handler: Arc<HandlerT>,
    phantom: PhantomData<tokio::sync::Mutex<(Upd, NextUpd, Rest, Err)>>,
}

impl<ParserT, Upd, NextUpd, Rest, Err, HandlerT>
    ParserHandler<ParserT, Upd, NextUpd, Rest, Err, HandlerT>
where
    ParserT: Parser<Upd, NextUpd, Rest>,
    HandlerT: Handler<DispatcherContext<NextUpd>, Err>,
{
    pub fn new<H>(parser: ParserT, handler: H) -> Self
    where
        H: IntoHandler<HandlerT>,
    {
        ParserHandler {
            parser: Arc::new(parser),
            handler: Arc::new(handler.into_handler()),
            phantom: PhantomData,
        }
    }
}

impl<ParserT, Upd, Err, NextUpd, Rest, HandlerT> Handler<DispatcherContext<Upd>, Err>
    for ParserHandler<ParserT, Upd, NextUpd, Rest, Err, HandlerT>
where
    Err: 'static,
    NextUpd: Send,
    Rest: Send,
    ParserT: Parser<Upd, NextUpd, Rest> + Send + Sync + 'static,
    Upd: RecombineFrom<ParserT, NextUpd, Rest> + Send + 'static,
    DispatcherContext<Upd>: RecombineFrom<ParserT, DispatcherContext<NextUpd>, Rest>,
    HandlerT: Handler<DispatcherContext<NextUpd>, Err> + Send + Sync + 'static,
{
    fn handle(&self, cx: DispatcherContext<Upd>) -> HandleFuture<Err, DispatcherContext<Upd>> {
        let parser = self.parser.clone();
        let handler = self.handler.clone();

        Box::pin(async move {
            match cx.parse_upd(parser.as_ref()) {
                Ok(ParserOut { data: cx, rest }) => match handler.handle(cx).await {
                    Ok(res) => Ok(res),
                    Err(next) => {
                        let cx = <DispatcherContext<Upd>>::recombine(ParserOut::new(next, rest));
                        Err(cx)
                    }
                },
                Err(upd) => Err(upd),
            }
        })
    }
}

pub struct ParserOut<T, Rest> {
    pub data: T,
    pub rest: Rest,
}

impl<T, Rest> ParserOut<T, Rest> {
    pub fn new(data: T, rest: Rest) -> Self {
        ParserOut { data, rest }
    }

    pub fn into_inner(self) -> (T, Rest) {
        (self.data, self.rest)
    }
}

pub trait Parser<From, To, Rest> {
    fn parse(&self, from: From) -> Result<ParserOut<To, Rest>, From>;
}

impl<F, From, To, Rest> Parser<From, To, Rest> for F
where
    F: Fn(From) -> Result<ParserOut<To, Rest>, From>,
    From: RecombineFrom<F, To, Rest>,
{
    fn parse(&self, from: From) -> Result<ParserOut<To, Rest>, From> {
        self(from)
    }
}

pub trait RecombineFrom<Parser, From, Rest> {
    fn recombine(info: ParserOut<From, Rest>) -> Self;
}
