package uniffi.proton_mail_uniffi

import android.content.Context

object RustInit {
    init { System.loadLibrary("proton_mail_uniffi") }

    @JvmStatic
    external fun init_tls()
}