use log::*;

use rusoto_sqs::Message as SqsMessage;
use rusoto_s3::{S3, GetObjectRequest};
use crate::event_decoder::PayloadDecoder;
use std::marker::PhantomData;
use aws_lambda_events::event::s3::S3Event;
use futures::compat::Future01CompatExt;
use std::error::Error;
use std::io::Read;
use async_trait::async_trait;
use std::time::Duration;

#[async_trait]
pub trait PayloadRetriever<T> {
    async fn retrieve_event(&mut self, msg: &SqsMessage) -> Result<T, Box< dyn Error>>;
}

#[derive(Clone)]
pub struct S3PayloadRetriever<S, D, E>
    where S: S3 + Clone + Send + Sync + 'static,
          D: PayloadDecoder<E> + Clone + Send + 'static,
          E: Send + 'static
{
    s3: S,
    decoder: D,
    phantom: PhantomData<E>
}

impl<S, D, E> S3PayloadRetriever<S, D, E>
    where S: S3 + Clone + Send + Sync + 'static,
          D: PayloadDecoder<E> + Clone + Send + 'static,
          E: Send + 'static
{
    pub fn new(s3: S, decoder: D) -> Self {
        Self {s3, decoder, phantom: PhantomData}
    }
}


#[async_trait]
impl<S, D, E> PayloadRetriever<E> for S3PayloadRetriever<S, D, E>
    where S: S3 + Clone + Send + Sync + 'static,
          D: PayloadDecoder<E> + Clone + Send + 'static,
          E: Send + 'static
{
    async fn retrieve_event(&mut self, msg: &SqsMessage) -> Result<E, Box<dyn Error>> {
        let body = msg.body.as_ref().unwrap();
        info!("Got body from message: {}", body);
        let event: S3Event = serde_json::from_str(body)?;

        let record = &event.records[0].s3;

        let s3_data = self.s3.get_object(
            GetObjectRequest {
                bucket: record.bucket.name.clone().unwrap(),
                key: record.object.key.clone().unwrap(),
                ..Default::default()
            }
        ).with_timeout(Duration::from_secs(2)).compat().await?;

        let prealloc = if record.object.size < 1024 {
            1024
        } else {
            record.object.size as usize
        };

        info!("Retrieved s3 payload with size : {:?}", prealloc);

        let mut body = Vec::with_capacity(prealloc);
        s3_data.body.unwrap().into_async_read().read_to_end(&mut body)?;
        info!("Read s3 payload body");
        self.decoder.decode(body)
    }
}
