use std::process::ExitCode;

use ncx_video_agent::{
    resolve_paid_seedance_prereqs, P1ExternalConfig, TosConfig, VideoAgentError,
};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("p1 paid config preflight failed: {err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> ncx_video_agent::Result<()> {
    let (tos_config, ark_key) = resolve_paid_seedance_prereqs(TosConfig::from_env, || {
        P1ExternalConfig::load()
            .ark_api_key
            .map(|setting| setting.value)
            .ok_or_else(|| {
                VideoAgentError::Ark(
                    "missing ARK_API_KEY, NANOCODEX_ARK_API_KEY, or ncx-config ark_api_key"
                        .to_string(),
                )
            })
    })?;

    println!("PASS P1 paid config preflight");
    println!("tos_endpoint: {}", tos_config.endpoint);
    println!("tos_bucket: {}", tos_config.bucket);
    println!("tos_region: {}", tos_config.region);
    println!("ark_api_key: configured ({} bytes)", ark_key.len());
    Ok(())
}
