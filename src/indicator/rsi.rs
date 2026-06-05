pub fn rsi(prices:&Vec<f64>, period:usize)->Vec<Option<f64>>{

    let mut rsis=Vec::new();

    if prices.len()<=period{
        return vec![];
    }

    rsis.resize(period,None);

    let mut gain=0.0;
    let mut loss=0.0;

    for i in 1..=period{

        let diff=prices[i]-prices[i-1];

        if diff>0.0{
            gain+=diff;
        }else{
            loss+=-diff;
        }
    }

    let mut avg_gain=gain/period as f64;
    let mut avg_loss=loss/period as f64;

    if avg_loss==0.0{
        rsis.push(Some(100.0));
    }else{

        let rs=avg_gain/avg_loss;
        rsis.push(Some(100.0-(100.0/(1.0+rs))));
    }

    for i in period+1..prices.len(){

        let diff=prices[i]-prices[i-1];

        let gain=if diff>0.0{diff}else{0.0};
        let loss=if diff<0.0{-diff}else{0.0};

        avg_gain=(avg_gain*(period as f64-1.0)+gain)/period as f64;
        avg_loss=(avg_loss*(period as f64-1.0)+loss)/period as f64;

        if avg_loss==0.0{

            rsis.push(Some(100.0));

        }else{

            let rs=avg_gain/avg_loss;

            let rsi=100.0-(100.0/(1.0+rs));

            rsis.push(Some(rsi));
        }
    }

    rsis
}