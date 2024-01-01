pub(crate) use xitca_http::{
    body::{NoneBody, Once},
    error::BodyError,
};

#[cfg(any(feature = "http1", feature = "http2", feature = "http3"))]
pub(crate) use xitca_http::body::BodySize;

use core::{
    fmt,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};

use futures_core::stream::Stream;
use pin_project_lite::pin_project;

use crate::bytes::Bytes;

#[allow(clippy::large_enum_variant)]
pub enum ResponseBody<'c> {
    #[cfg(feature = "http1")]
    H1(crate::h1::body::ResponseBody<crate::connection::ConnectionWithKey<'c>>),
    #[cfg(feature = "http2")]
    H2(crate::h2::body::ResponseBody),
    #[cfg(feature = "http3")]
    H3(crate::h3::body::ResponseBody),
    // TODO: add http1 eof resposne body variant.
    #[allow(dead_code)]
    Eof(PhantomData<&'c ()>),
}

impl ResponseBody<'_> {
    pub(crate) fn destroy_on_drop(&mut self) {
        #[cfg(feature = "http1")]
        if let Self::H1(ref mut body) = *self {
            body.conn().destroy_on_drop()
        }
    }

    pub(crate) fn can_destroy_on_drop(&mut self) -> bool {
        #[cfg(feature = "http1")]
        if let Self::H1(ref mut body) = *self {
            return body.conn().is_destroy_on_drop();
        }

        false
    }
}

impl fmt::Debug for ResponseBody<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            #[cfg(feature = "http1")]
            Self::H1(_) => write!(f, "ResponseBody::H1(..)"),
            #[cfg(feature = "http2")]
            Self::H2(_) => write!(f, "ResponseBody::H2(..)"),
            #[cfg(feature = "http3")]
            Self::H3(_) => write!(f, "ResponseBody::H3(..)"),
            Self::Eof(_) => write!(f, "ResponseBody::Eof"),
        }
    }
}

impl Stream for ResponseBody<'_> {
    type Item = Result<Bytes, BodyError>;

    fn poll_next(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.get_mut() {
            #[cfg(feature = "http1")]
            Self::H1(body) => Pin::new(body).poll_next(_cx),
            #[cfg(feature = "http2")]
            Self::H2(body) => Pin::new(body).poll_next(_cx),
            #[cfg(feature = "http3")]
            Self::H3(body) => Pin::new(body).poll_next(_cx),
            Self::Eof(_) => Poll::Ready(None),
        }
    }
}

/// type erased stream body.
pub struct BoxBody(Pin<Box<dyn Stream<Item = Result<Bytes, BodyError>> + Send + 'static>>);

impl Default for BoxBody {
    fn default() -> Self {
        Self::new(NoneBody::default())
    }
}

impl BoxBody {
    #[inline]
    pub fn new<B, E>(body: B) -> Self
    where
        B: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: Into<BodyError>,
    {
        Self(Box::pin(BoxStreamMapErr { body }))
    }
}

impl Stream for BoxBody {
    type Item = Result<Bytes, BodyError>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.get_mut().0.as_mut().poll_next(cx)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

pin_project! {
    struct BoxStreamMapErr<B> {
        #[pin]
        body: B
    }
}

impl<B, T, E> Stream for BoxStreamMapErr<B>
where
    B: Stream<Item = Result<T, E>>,
    E: Into<BodyError>,
{
    type Item = Result<T, BodyError>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().body.poll_next(cx).map_err(Into::into)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        self.body.size_hint()
    }
}
