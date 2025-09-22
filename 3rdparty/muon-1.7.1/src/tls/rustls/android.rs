use jni::objects::{JClass, JObject};

/// Initialize the rustls-platform-verifier library.
#[deprecated = "unsafe and error prone: prefer `muon::tls::init_external`"]
pub fn java_init(env: &mut JNIEnv, _: JClass, context: JObject) -> sys::jboolean {
    match rustls_platform_verifier::android::init_hosted(env, context) {
        Ok(()) => {
            info!("rustls-platform-verifier initialized");
            true as sys::jboolean
        }

        Err(e) => {
            error!(%e, "failed to initialize rustls-platform-verifier");
            false as sys::jboolean
        }
    }
}

export! {
    /// Public re-exports of JNI-related methods and types.
    mod public (as pub) {
        pub use jni::*;
        pub use rustls_platform_verifier::android::*;
    }
}
