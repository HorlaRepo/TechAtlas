use std::error::Error;
use techatlas_api::openapi::ApiDoc;
use utoipa::OpenApi;

fn main() -> Result<(), Box<dyn Error>> {
    print!("{}", ApiDoc::openapi().to_yaml()?);
    Ok(())
}
