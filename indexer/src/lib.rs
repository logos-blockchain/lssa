use bedrock_client::BedrockClient;
use futures::Stream;
use url::Url;

pub struct IndexerCore {
    pub bedrock_client: BedrockClient, 
    pub bedrock_url: Url,
}

impl IndexerCore {
    pub async fn subscribe_block_stream(&self) -> Result<impl Stream<Item = BlockInfo>, bedrock_client::Error> {
        self.bedrock_client.0.get_lib_stream(self.bedrock_url).await
    }
}