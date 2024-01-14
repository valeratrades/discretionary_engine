use anyhow::Result;
use reqwest::Url;
//TODO!!!!!!!!: signed request implementation

pub async fn signed_request(url: String, key: String, secret: String) -> Result<f32> {
	//let client = reqwest::Client::new();
	//let response = client
	//	.get(url)
	//	.header("X-MBX-APIKEY", key)
	//	.send()
	//	.await?
	//	.text()
	//	.await?;
	//
	//println!("{}", response);

	Ok(1.0) //dbg
}
