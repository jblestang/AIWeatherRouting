use eccodes::{CodesFile, ProductKind, KeyRead, DynamicKeyType, FallibleIterator};
use std::collections::HashSet;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path_str = "data/arpege_sample_small.grib2";
    println!("Opening {}...", path_str);
    
    let mut file = CodesFile::new_from_file(path_str, ProductKind::GRIB)?;
    let mut iter = file.ref_message_iter();
    
    let mut unique_vars = HashSet::new();
    let mut count = 0;
    
    // Trap the iteration so a Premature EOF doesn't panic the whole run
    loop {
        match iter.next() {
            Ok(Some(message)) => {
                count += 1;
                if count > 500 {
                    break;
                }
                
                let short_name = match message.read_key_dynamic("shortName") {
                    Ok(DynamicKeyType::Str(name)) => name,
                    _ => "unknown".to_string()
                };
                
                let level_type = match message.read_key_dynamic("typeOfLevel") {
                    Ok(DynamicKeyType::Str(name)) => name,
                    _ => "unknown".to_string()
                };
                
                unique_vars.insert(format!("{} at {}", short_name, level_type));
            },
            Ok(None) => break, // Clean EOF
            Err(e) => {
                println!("Stopped parsing early due to error (likely truncated slice): {:?}", e);
                break;
            }
        }
    }
    
    println!("Found variables in first {} messages:", count);
    for var in unique_vars {
        println!(" - {}", var);
    }
    
    Ok(())
}
