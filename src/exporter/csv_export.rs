use crate::model::stock::Meta;

use std::fs::File;
use std::io::Write;

pub fn export_csv(

    date:&str,

    stocks:&Vec<Meta>

){

    let filename=format!("export_{}.csv",date);

    let mut file=File::create(filename).unwrap();

    writeln!(file,"code,name,price,volume").unwrap();

    for s in stocks{

        writeln!(
            file,
            "{},{},{},{}",
            s.code,
            s.name,
            s.price,
            s.volume
        ).unwrap();
    }
}