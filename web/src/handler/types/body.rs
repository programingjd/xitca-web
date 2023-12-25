//! type extractor for request body stream.

use core::{cmp, convert::Infallible, fmt, future::poll_fn, pin::pin};

use std::error;

use crate::{
    body::BodyStream,
    bytes::{Bytes, BytesMut},
    context::WebContext,
    error::{error_from_service, forward_blank_bad_request, Error},
    handler::{FromRequest, Responder},
    http::WebResponse,
};

use super::header::{self, HeaderRef};

pub struct Body<B>(pub B);

impl<'a, 'r, C, B> FromRequest<'a, WebContext<'r, C, B>> for Body<B>
where
    B: BodyStream + Default,
{
    type Type<'b> = Body<B>;
    type Error = Error<C>;

    #[inline]
    async fn from_request(ctx: &'a WebContext<'r, C, B>) -> Result<Self, Self::Error> {
        Ok(Body(ctx.take_body_ref()))
    }
}

/// helper type for limiting body size.
/// when LIMIT > 0 body size is limited to LIMIT in bytes.
/// when LIMIT == 0 body size is unlimited.
pub struct Limit<const LIMIT: usize>;

macro_rules! from_bytes_impl {
    ($type: ty, $original: tt) => {
        impl<'a, 'r, C, B, const LIMIT: usize> FromRequest<'a, WebContext<'r, C, B>> for ($type, Limit<LIMIT>)
        where
            B: BodyStream + Default,
        {
            type Type<'b> = ($type, Limit<LIMIT>);
            type Error = Error<C>;

            async fn from_request(ctx: &'a WebContext<'r, C, B>) -> Result<Self, Self::Error> {
                let limit = HeaderRef::<'a, { header::CONTENT_LENGTH }>::from_request(ctx)
                    .await
                    .ok()
                    .and_then(|header| header.to_str().ok().and_then(|s| s.parse().ok()))
                    .map(|len| cmp::min(len, LIMIT))
                    .unwrap_or_else(|| LIMIT);

                let body = ctx.take_body_ref();

                let mut body = pin!(body);

                let mut buf = <$type>::new();

                while let Some(chunk) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
                    let chunk = chunk.map_err(Into::into)?;
                    buf.extend_from_slice(chunk.as_ref());
                    if limit > 0 && buf.len() > limit {
                        return Err(Error::from(BodyOverFlow {
                            limit,
                            len: buf.len(),
                        }));
                    }
                }

                Ok((buf, Limit))
            }
        }

        from_bytes_impl!($type);
    };
    ($type: ty) => {
        impl<'a, 'r, C, B> FromRequest<'a, WebContext<'r, C, B>> for $type
        where
            B: BodyStream + Default,
        {
            type Type<'b> = $type;
            type Error = Error<C>;

            #[inline]
            async fn from_request(ctx: &'a WebContext<'r, C, B>) -> Result<Self, Self::Error> {
                <($type, Limit<0>)>::from_request(ctx)
                    .await
                    .map(|(bytes, _)| bytes)
            }
        }
    };
}

from_bytes_impl!(BytesMut, _);
from_bytes_impl!(Vec<u8>, _);

impl<'a, 'r, C, B, const LIMIT: usize> FromRequest<'a, WebContext<'r, C, B>> for (Bytes, Limit<LIMIT>)
where
    B: BodyStream + Default,
{
    type Type<'b> = (Bytes, Limit<LIMIT>);
    type Error = Error<C>;

    #[inline]
    async fn from_request(ctx: &'a WebContext<'r, C, B>) -> Result<Self, Self::Error> {
        <(BytesMut, Limit<LIMIT>)>::from_request(ctx)
            .await
            .map(|(bytes, limit)| (bytes.into(), limit))
    }
}

from_bytes_impl!(Bytes);

macro_rules! responder_impl {
    ($type: ty) => {
        impl<'r, C, B> Responder<WebContext<'r, C, B>> for $type {
            type Response = WebResponse;
            type Error = Infallible;

            #[inline]
            async fn respond(self, ctx: WebContext<'r, C, B>) -> Result<Self::Response, Self::Error> {
                Ok(ctx.into_response(self))
            }

            fn map(self, res: Self::Response) -> Result<Self::Response, Self::Error> {
                Ok(res.map(|_| self.into()))
            }
        }
    };
}

responder_impl!(Bytes);
responder_impl!(BytesMut);
responder_impl!(Vec<u8>);

#[derive(Debug)]
pub struct BodyOverFlow {
    limit: usize,
    len: usize,
}

impl fmt::Display for BodyOverFlow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "body size over flow. limit: {}, actual_len: {}",
            self.limit, self.len
        )
    }
}

impl error::Error for BodyOverFlow {}

error_from_service!(BodyOverFlow);
forward_blank_bad_request!(BodyOverFlow);
