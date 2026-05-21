use curiosity_audio::{ManualSmokeCheck, ManualSmokeStatus};

fn main() {
    let result = ManualSmokeCheck::macos_placeholder().run_without_hardware();
    println!("{:?}: {}", result.status, result.message);

    if result.status != ManualSmokeStatus::Passed {
        std::process::exit(2);
    }
}
