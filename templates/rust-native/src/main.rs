use infinity_rs::simconnect::SimConnect;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let sim = SimConnect::open("desktop-native")?;
    println!("Connected to sim");
    Ok(())
}
