use core::{future::poll_fn, pin::Pin};

use std::io;

use futures_core::stream::Stream;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use xitca_http::{body::BodySize, bytes::Buf, h1::proto::codec::TransferCoding};

use crate::{
    body::BodyError,
    bytes::{Bytes, BytesMut},
    date::DateTimeHandle,
    h1::Error,
    http::{
        header::{HeaderValue, EXPECT, HOST},
        Method, Request, Response, StatusCode,
    },
};

use super::context::Context;

pub(crate) async fn send<S, B, E>(
    stream: &mut S,
    date: DateTimeHandle<'_>,
    req: &mut Request<B>,
) -> Result<(Response<()>, BytesMut, Vec<u8>, TransferCoding, bool), Error>
where
    S: AsyncRead + AsyncWrite + Unpin,
    B: Stream<Item = Result<Bytes, E>> + Unpin,
    BodyError: From<E>,
{
    let mut stream = Pin::new(stream);
    let mut buf = BytesMut::new();

    if !req.headers().contains_key(HOST) {
        if let Some(host) = req.uri().host() {
            buf.reserve(host.len() + 5);
            buf.extend_from_slice(host.as_bytes());

            if let Some(port) = req.uri().port() {
                let port = port.as_str();
                match port {
                    "80" | "443" => {}
                    _ => {
                        buf.extend_from_slice(b":");
                        buf.extend_from_slice(port.as_bytes());
                    }
                }
            }

            let val = HeaderValue::from_maybe_shared(buf.split().freeze()).unwrap();
            req.headers_mut().insert(HOST, val);
        }
    }

    let mut is_expect = req.headers().contains_key(EXPECT);

    if is_expect {
        match BodySize::from_stream(req.body()) {
            // remove expect header if there is no body.
            BodySize::None | BodySize::Sized(0) => {
                let crate::http::header::Entry::Occupied(entry) = req.headers_mut().entry(EXPECT) else {
                    unreachable!()
                };
                entry.remove_entry();
                is_expect = false;
            }
            _ => {}
        }
    }

    // TODO: make const generic params configurable.
    let mut ctx = Context::<128>::new(&date);

    // encode request head and return transfer encoding for request body
    let encoder = ctx.encode_head(&mut buf, req)?;

    // it's important to call set_head_method after encode_head. Context would remove http body it encodes/decodes
    // for head http method.
    if *req.method() == Method::HEAD {
        ctx.set_head_method();
    }

    write_all_buf(stream.as_mut(), &mut buf).await?;

    let mut chunk = vec![0; 4096];

    if is_expect {
        poll_fn(|cx| stream.as_mut().poll_flush(cx)).await?;

        loop {
            if let Some((res, mut decoder)) = try_read_response(stream.as_mut(), &mut buf, &mut chunk, &mut ctx).await?
            {
                if res.status() == StatusCode::CONTINUE {
                    break;
                }

                let is_close = ctx.is_connection_closed();

                if ctx.is_head_method() {
                    decoder = TransferCoding::eof();
                }

                return Ok((res, buf, chunk, decoder, is_close));
            }
        }
    }

    // TODO: concurrent read write is needed in case server decide to do two way
    // streaming with very large body surpass socket buffer size.
    // (In rare case the server could starting streaming back response without read all the request body)

    // try to send request body.
    // continue to read response no matter the outcome.
    if send_body(stream.as_mut(), encoder, req.body_mut(), &mut buf)
        .await
        .is_err()
    {
        // an error indicate connection should be closed.
        ctx.set_close();
        // clear the buffer as there could be unfinished request data inside.
        buf.clear();
    }

    // read response head and get body decoder.
    loop {
        if let Some((res, mut decoder)) = try_read_response(stream.as_mut(), &mut buf, &mut chunk, &mut ctx).await? {
            // check if server sent connection close header.

            // *. If send_body function produces error, Context has already set
            // connection type to ConnectionType::CloseForce. We trust the server response
            // to not produce another connection type that override it to any variant
            // other than ConnectionType::Close in this case and only this case.

            let is_close = ctx.is_connection_closed();

            if ctx.is_head_method() {
                decoder = TransferCoding::eof();
            }

            return Ok((res, buf, chunk, decoder, is_close));
        }
    }
}

async fn send_body<S, B, E>(
    mut stream: Pin<&mut S>,
    mut encoder: TransferCoding,
    body: &mut B,
    buf: &mut BytesMut,
) -> Result<(), Error>
where
    S: AsyncWrite,
    B: Stream<Item = Result<Bytes, E>> + Unpin,
    BodyError: From<E>,
{
    if !encoder.is_eof() {
        let mut body = Pin::new(body);

        // poll request body and encode.
        while let Some(bytes) = poll_fn(|cx| body.as_mut().poll_next(cx)).await {
            let bytes = bytes.map_err(BodyError::from)?;
            encoder.encode(bytes, buf);
            // we are not in a hurry here so write before handling next chunk.
            write_all_buf(stream.as_mut(), buf).await?;
        }

        // body is finished. encode eof and clean up.
        encoder.encode_eof(buf);

        write_all_buf(stream.as_mut(), buf).await?;
    }

    poll_fn(|cx| stream.as_mut().poll_flush(cx)).await.map_err(Into::into)
}

async fn write_all_buf<S>(mut stream: Pin<&mut S>, buf: &mut BytesMut) -> io::Result<()>
where
    S: AsyncWrite,
{
    while buf.has_remaining() {
        let n = poll_fn(|cx| stream.as_mut().poll_write(cx, buf.chunk())).await?;
        buf.advance(n);
        if n == 0 {
            return Err(io::Error::from(io::ErrorKind::WriteZero));
        }
    }
    Ok(())
}

async fn try_read_response<S>(
    mut stream: Pin<&mut S>,
    buf: &mut BytesMut,
    chunk: &mut [u8],
    ctx: &mut Context<'_, '_, 128>,
) -> Result<Option<(Response<()>, TransferCoding)>, Error>
where
    S: AsyncRead,
{
    let mut b = ReadBuf::new(chunk);
    poll_fn(|cx| stream.as_mut().poll_read(cx, &mut b)).await?;
    let filled = b.filled();

    if filled.is_empty() {
        return Err(Error::from(io::Error::from(io::ErrorKind::UnexpectedEof)));
    }

    buf.extend_from_slice(filled);

    ctx.decode_head(buf).map_err(Into::into)
}
