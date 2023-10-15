use crate::{
    body::BodyStream,
    handler::{
        error::{ExtractError, _ParseError},
        FromRequest,
    },
    request::WebRequest,
};

impl<'r, C, B> FromRequest<WebRequest<'r, C, B>> for String
where
    B: BodyStream + Default,
{
    type Type<'b> = String;
    type Error = ExtractError<B::Error>;

    #[inline]
    async fn from_request<'a>(req: &'a WebRequest<'r, C, B>) -> Result<Self::Type<'a>, Self::Error> {
        let vec = Vec::from_request(req).await?;
        Ok(String::from_utf8(vec).map_err(|e| _ParseError::String(e.utf8_error()))?)
    }
}
