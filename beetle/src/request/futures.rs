//! Futures and errors returned from foreign type impl of [`FromRequest`].
use bytes::Bytes;
use std::{
    convert::Infallible,
    future::{Ready, ready},
    io,
    string::FromUtf8Error,
};

use super::{Collect, FromRequest, FromRequestParts, Parts, Request};
use crate::{
    futures::{FutureExt, Map, MapErr, MapOk, TryFutureExt},
    helpers::BadRequest,
    http::Method,
};

// NOTE:
// using Pin<Box> in association type is worth it instead of impl Future,
// because it can be referenced externally

macro_rules! from_parts {
    ($self:ty, $($id:ident = $t:ty;)* ($parts:pat) => $body: expr) => {
        impl FromRequestParts for $self {
            $(type $id = $t;)*

            fn from_request_parts($parts: &mut Parts) -> Self::Future {
                $body
            }
        }
    };
}

from_parts! {
    Method,
    Error = Infallible;
    Future = Ready<Result<Self,Infallible>>;
    (parts) => ready(Ok(parts.method()))
}

macro_rules! from_request {
    ($self:ty, $($id:ident = $t:ty;)* ($req:pat) => $body: expr) => {
        impl FromRequest for $self {
            $(type $id = $t;)*

            fn from_request($req: Request) -> Self::Future {
                $body
            }
        }
    };
}

/// anything that implement `FromRequestParts` also implement `FromRequest`
impl<F> FromRequest for F
where
    F: FromRequestParts
{
    type Error = <F as FromRequestParts>::Error;
    type Future = <F as FromRequestParts>::Future;

    fn from_request(req: Request) -> Self::Future {
        Self::from_request_parts(&mut req.into_parts().0)
    }
}

from_request! {
    (),
    Error = Infallible;
    Future = Ready<Result<Self,Infallible>>;
    (_) => ready(Ok(()))
}

from_request! {
    Request,
    Error = Infallible;
    Future = Ready<Result<Self,Infallible>>;
    (req) => ready(Ok(req))
}

from_request! {
    Bytes,
    Error = BadRequest<io::Error>;
    Future = MapErr<Collect, fn(io::Error) -> BadRequest<io::Error>>;
    (req) => req.into_body().collect().map_err(BadRequest)
}

from_request! {
    Vec<u8>,
    Error = BadRequest<io::Error>;
    Future = MapOk<<Bytes as FromRequest>::Future, fn(Bytes) -> Vec<u8>>;
    (req) => Bytes::from_request(req).map_ok(Into::into as _)
}

from_request! {
    String,
    Error = BadRequest<BytesUtf8Error>;
    Future = Map<
        <Bytes as FromRequest>::Future,
        fn(Result<Bytes, BadRequest<io::Error>>) -> Result<String, BadRequest<BytesUtf8Error>>,
    >;
    (req) => Bytes::from_request(req).map(map_to_string)
}

fn map_to_string(result: Result<Bytes, BadRequest<io::Error>>) -> Result<String, BadRequest<BytesUtf8Error>> {
    String::from_utf8(result.map_err(BadRequest::map)?.into())
        .map_err(Into::<BytesUtf8Error>::into)
        .map_err(Into::into)
}

#[derive(Debug)]
pub enum BytesUtf8Error {
    Io(io::Error),
    FromUtf8(FromUtf8Error),
}

impl std::error::Error for BytesUtf8Error { }

impl std::fmt::Display for BytesUtf8Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use std::fmt::Display;
        match self {
            BytesUtf8Error::Io(error) => Display::fmt(error, f),
            BytesUtf8Error::FromUtf8(error) => Display::fmt(error, f),
        }
    }
}

impl From<io::Error> for BytesUtf8Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<FromUtf8Error> for BytesUtf8Error {
    fn from(value: FromUtf8Error) -> Self {
        Self::FromUtf8(value)
    }
}

