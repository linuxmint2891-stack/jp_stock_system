use polars::prelude::*;
use std::fs;

fn main() -> PolarsResult<()> {
    let path = "./data/daily_2024-04-11.json";
    let file = fs::File::open(path).map_err(|e| PolarsError::ComputeError(e.to_string().into()))?;
    
    let df = JsonReader::new(file).finish()?;
    println!("Original DF schema: {:?}", df.schema());
    
    let exploded = df.explode(["daily_quotes"])?;
    println!("Exploded DF schema: {:?}", exploded.schema());
    
    let unnested = exploded.unnest(["daily_quotes"])?;
    println!("Unnested DF schema: {:?}", unnested.schema());
    
    println!("Unnested shape: {:?}", unnested.shape());
    
    Ok(())
}
