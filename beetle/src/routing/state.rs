use crate::{
    request::Request,
    service::{HttpService, Service},
};

pub struct State<T, S> {
    state: T,
    inner: S,
}

impl<T, S> State<T, S> {
    pub fn new(state: T, inner: S) -> Self {
        Self { state, inner }
    }
}

impl<T, S> Service<Request> for State<T, S>
where
    T: Clone + Send + Sync + 'static,
    S: HttpService,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn call(&self, mut req: Request) -> Self::Future {
        req.extensions_mut().insert(self.state.clone());
        self.inner.call(req)
    }
}

