pub fn sma(
    prices:&Vec<f64>,
    period:usize
)->Vec<Option<f64>>{

    let mut result=Vec::new();

    for i in 0..prices.len(){

        if i+1<period{

            result.push(None);

        }else{

            let start=i+1-period;

            let slice=&prices[start..=i];

            let sum:f64=slice.iter().sum();

            result.push(Some(sum/period as f64));

        }

    }

    result
}