# async-rusoto_core

An rusoto_core alternative for async-std

## Usage

```rust
use async_rusoto_core::HttpClient;
use http_client::isahc::IsahcClient;
use rusoto_core;
use rusoto_credential::EnvironmentProvider;
use rusoto_s3::{ListBucketsOutput, S3Client, S3};

let isahc = IsahcClient::new();
let req_dispatcher = HttpClient::new(isahc);
let cred_provider = EnvironmentProvider::default();
let apne1 = rusoto_core::Region::ApNortheast1;
let s3 = S3Client::new_with(req_dispatcher, cred_provider, apne1);
let ListBucketsOutput { buckets, .. } = s3.list_buckets().await.unwrap();
let buckets = buckets.unwrap();
for bucket in buckets {
    println!("{}", bucket.name.unwrap());
}
```