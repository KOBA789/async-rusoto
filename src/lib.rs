use async_std::future::timeout as fut_timeout;
use bytes::BytesMut;
use futures_util::{stream, AsyncReadExt as _, FutureExt as _, TryFutureExt as _, TryStreamExt};
use http;
use http_client::Request;
use http_types::{Body, Method, Url};
use rusoto_core::{
    request::{HttpDispatchError, HttpResponse},
    DispatchSignedRequest,
};
use rusoto_signature::{ByteStream, SignedRequest, SignedRequestPayload};
use std::convert::TryFrom;
use std::fmt::Display;
use std::future::Future;
use std::iter::FromIterator;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

pub struct HttpClient<H>(Arc<H>);

impl<H> HttpClient<H>
where
    H: http_client::HttpClient,
    <H as http_client::HttpClient>::Error: Display,
{
    // TODO: Make it configurable
    //       or improve buffering algorithm
    const BUFFER_SIZE: usize = 1024 * 16;

    pub fn new(client: H) -> Self {
        HttpClient(Arc::new(client))
    }
}

impl<H> DispatchSignedRequest for HttpClient<H>
where
    H: http_client::HttpClient,
    <H as http_client::HttpClient>::Error: Display,
{
    fn dispatch(
        &self,
        request: SignedRequest,
        timeout: Option<Duration>,
    ) -> Pin<Box<dyn Future<Output = Result<HttpResponse, HttpDispatchError>> + Send>> {
        let client = self.0.clone();
        async move {
            let h1_method = Method::try_from(request.method().as_ref()).map_err(display_error)?;
            let mut final_uri = format!(
                "{}://{}{}",
                request.scheme(),
                request.hostname(),
                request.canonical_path()
            );
            if !request.canonical_query_string().is_empty() {
                final_uri = final_uri + &format!("?{}", request.canonical_query_string());
            }
            let h1_url = Url::parse(&final_uri).map_err(display_error)?;
            let mut h1_request = Request::new(h1_method, h1_url);
            for (name, values) in request.headers().iter() {
                for value in values {
                    let str_value = std::str::from_utf8(value).map_err(display_error)?;
                    h1_request
                        .append_header(name.as_str(), str_value)
                        .map_err(display_error)?;
                }
            }

            let h1_body = if let Some(p) = request.payload {
                match p {
                    SignedRequestPayload::Buffer(bytes) => Body::from(bytes.to_vec()),
                    SignedRequestPayload::Stream(stream) => {
                        Body::from_reader(TryStreamExt::into_async_read(stream), None)
                    }
                }
            } else {
                Body::empty()
            };
            h1_request.set_body(h1_body);

            let req_fut = client.send(h1_request);
            let mut resp = match timeout {
                Some(dur) => fut_timeout(dur, req_fut.map_err(display_error))
                    .await
                    .map_err(display_error)??,
                None => req_fut.await.map_err(display_error)?,
            };
            let status = http::StatusCode::from_u16(resp.status().into()).map_err(display_error)?;
            let resp_body_stream = stream::try_unfold(resp.take_body(), |mut body| async move {
                let mut buf = BytesMut::with_capacity(Self::BUFFER_SIZE);
                buf.resize(Self::BUFFER_SIZE, 0);
                let len = body.read(&mut buf).await?;
                if len == 0 {
                    return Ok(None);
                }
                buf.truncate(len);
                Ok(Some((buf.freeze(), body)))
            });
            let resp_body = ByteStream::new(resp_body_stream);
            let kv_pairs = resp.iter().flat_map(|(key, values)| {
                let header_name =
                    http::header::HeaderName::from_bytes(key.as_str().as_bytes()).unwrap();
                values
                    .iter()
                    .map(move |value| (header_name.clone(), value.to_string()))
            });
            let resp_header_map = http::HeaderMap::from_iter(kv_pairs);

            Ok(HttpResponse {
                status,
                body: resp_body,
                headers: resp_header_map,
            })
        }
        .boxed()
    }
}

fn display_error<E: Display>(e: E) -> HttpDispatchError {
    HttpDispatchError::new(format!("{}", e))
}
