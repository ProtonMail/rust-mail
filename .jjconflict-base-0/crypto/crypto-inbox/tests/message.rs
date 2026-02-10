use proton_crypto_inbox::message::{DecryptableMessage, DecryptedBody, GettablePGPMessage};
use proton_crypto_inbox::proton_crypto::crypto::VerificationError;

mod common;
use common::{get_test_address_keys, get_test_public_address_keys};
use proton_crypto_inbox_mime::ProcessedBodyType;

use crate::common::{get_test_address_key_source, get_test_public_address_key_source};

pub const TEST_VERIFICATION_KEY_MIME: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

xjMEZf15lRYJKwYBBAHaRw8BAQdArPz06hKiOUYSVs6dbHpKSh63bW5/QyIF
qRvJ5wOALJnNMkx1a2FzIEJ1cmtoYWx0ZXIgPGtleXRyYW5zcGFyZW5jeW1h
aWxlckBnbWFpbC5jb20+wo8EExYIADcWIQSNEf53FU6EMmZs43pG8PpwjTNi
IAUCZf15lQUJBaOagAIbAwQLCQgHBRUICQoLBRYCAwEAAAoJEEbw+nCNM2Ig
aX0BANKGrENgM7nbpt5uORfaT5JLx695q1RgKDetm6bQhB1/AQDHvY3oha+e
abN+yKcOWKlvvNpbbbYzjunnrmfm7d+HDM44BGX9eZUSCisGAQQBl1UBBQEB
B0Aq4KRFu4d/XmR2UEGjsXeWCWvvKUkzsCR/wRDn8E/lRQMBCAfCfgQYFggA
JhYhBI0R/ncVToQyZmzjekbw+nCNM2IgBQJl/XmVBQkFo5qAAhsMAAoJEEbw
+nCNM2IgEzcBAPqEmyOcnbzbsGJaZ5uFEA3OfGH7anEg2xEbfZ0jxAh0AP9n
sO+JqQrVW5m3aGW4MRMFRjnC2DIHthThNQMw1bZpDQ==
=ziuc
-----END PGP PUBLIC KEY BLOCK-----
";

pub const TEST_DECRYPTION_KEY_MIME: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----

xYYEZSfovhYJKwYBBAHaRw8BAQdA6gS5mfVImh6ONhKgZGSVrLH4cdZaS9IW
6FhqYGWe2wr+CQMI7cZcc+SQB+tgAAAAAAAAAAAAAAAAAAAAAKEiVaK2iq+g
Y3+lmnRmmRZ4/HeC9UOoRmmFxHiHqFflv+bfqRD3hL2/+ayIG4MpahvRrnd0
ss0nbHVidXhAcHJvdG9uLmJsYWNrIDxsdWJ1eEBwcm90b24uYmxhY2s+wowE
EBYKAD4FgmUn6L4ECwkHCAmQVfYMqF9LlQEDFQgKBBYAAgECGQECmwMCHgEW
IQRJQPffztT8sMiZ4Y1V9gyoX0uVAQAAIOMA+wUpEGAm8SsDMt/tuaTSYrV/
DBsUzTYtFbzoBkT+dOLRAQDvZ4Z/YUn7mX71v0qXVTfGY5oLnY88Wuo9dySU
ns8kB8eLBGUn6L4SCisGAQQBl1UBBQEBB0DzvEDbVNT8WhIxijPVGHKGQ1Y3
s9Zw1i63nkkSnpLzNwMBCAf+CQMICODa4UCuLdlgAAAAAAAAAAAAAAAAAAAA
ABF+V4UBANv2UoEWSWPt2lltQkXnsXZ9rB5NkywVQwqc5vW/h3yx5vjZEY10
4jA3eSBo2bIaocJ4BBgWCAAqBYJlJ+i+CZBV9gyoX0uVAQKbDBYhBElA99/O
1PywyJnhjVX2DKhfS5UBAAASqQEA4qisiR8EHC6S7/EsUhS2uuin1tY0KQ0j
1jmrk+HHQugA/in2lPCiO/6RdSLXnbXnGj+7lP65+qrMXHb+mqBRdWsA
-----END PGP PRIVATE KEY BLOCK-----
";

const TEST_MESSAGE_BODY: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\nwV4Di5gBfuEszfESAQdAzv+eAfvm7tTd8GHvGn3Qsp2LhI2yjtKgSeg7uS69\nDV0w3DaikcJRSBvqQPWkkimzIdpyBIe4fzIaVERcUil0PTd+F8/zljGWTfNj\n29c030K90sBdATjoTBKarkG1Th7sllv1mC51vuxlvFateZmiLDNDeog6SdwM\n0YI9eKyT2+Wpyi9ehfw6HAwlMKDMY0ybFxhBCSpuWSZ9kIenGKJMym3MhkJM\nJu4J4F+PcZwO+katTJN4CnqyrGSOJYllECWqggZDdoF4nEm3G2LYI1W573Q6\no+fRqywqyPdHaqDiqviuL29RsqeG+Y+4TxQhXS2i4AfbhkBw1pv0fudTlNCu\nBSerK9SkpBKeDRxbfmmaRVPL0aFZjjwFYy0USg0JP0VEWClB0CCLiKhHvQsE\nUSy5VGT9ChsTRl2idtc2iUcfBUKiLT8JlAFfzFVW8WZgfpEEmUgSNS06/SQ/\ncaz1Mm9EF6xfkiBjxwDG7iEZSHIbzMCi\n=7AjW\n-----END PGP MESSAGE-----\n";
const TEST_EXPECTED_BODY: &str = r#"<div style="font-family: Arial, sans-serif; font-size: 14px;"><span>Test Attachment</span><br></div>
"#;

const TEST_MESSAGE_MIME: &str = "-----BEGIN PGP MESSAGE-----\nVersion: ProtonMail\n\n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\nVRx0RtmAQbLF/84EXzQJZxKg0mmnCg==\n=oKh2\n-----END PGP MESSAGE-----\n";

const TEST_EXPECTED_BODY_MIME: &str = r#"<!DOCTYPE html>
<html>
  <head>

    <meta http-equiv="content-type" content="text/html; charset=UTF-8">
  </head>
  <body>
    <p>This is a test <b>mime message from thunderbrid</b> with three
      attachments.<br>
    </p>
  </body>
</html>
"#;

const TEST_MESSAGE_BODY_GO: &str = "-----BEGIN PGP MESSAGE-----
Version: ProtonMail
Comment: https://protonmail.com

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
LloBMV1YNGsW45M+oQ==
=BRHT
-----END PGP MESSAGE-----
";

const TEST_DECRYPTION_KEY_GO: &str = "-----BEGIN PGP PRIVATE KEY BLOCK-----
Version: GopenPGP.js v3.1.2
Comment: https://openpgpjs.org

xcaGBFtq/A4BEAC0VKmsQb6HKMWwcpBNaPdEDYTUOQRdA9MvaOgRqJRmxYAG
+RVveeAjc/QuAwra2ePP91OibE1eY1ie3UfEF+X7IL+lB/alnUyg/sLW23CZ
csnSpQ8Wbs7pudIw3fOIxncFKKbnKE89QSKqHTH0u9TsxRZZ4yw7JgsGo5r5
WUuGOMn/G5PaffMaBmlXe8HbTmA9glrRvsrupRdBen913ZcaVtwt5CYCTgb6
WfnOKp5p4yHG8KfEeQtDJ6rhNJDewbcr9V0VUmbRjJDyRXPYc0eKi8kb9KIv
gW46oQVxkSOj1pCmxch0hXnTkSiVd2XM1sp4MeyfWbLOijjhN4g6gT34FM/B
+iwh81lbyF1eBOl2Hppz9QF90m5dPYvkTl+h0p2dbBriFT4RbqhuhYNgeaMH
cQciNZcfJbppC3zQjcYpVQj3Ie09IMuaRYiuI87EQCoVd/A0BZhP4nC5dWTp
eVUl2x1hYn66qIYBodw+TRc7mP28caWK7YpTQxS1BBRPseADMNdUbPW8d0t8
BcXG8fTQVgvZRNjkkg5/8ih9k0NoYqhW+Ix6Fjt4U/JhsYRnICmxoX3UmJ3Y
Zdm4G5FGB3/xi8uhltHtDwuhHeBeuv3No2K4uIz9sFNlYz6Nh7tTvlr4Xzvw
aB/Dd9Bxf/n/vp1CzLA1k7A1hQ4re/+BnvET1wARAQAB/gkDCAd1lurMBKVx
YKsG7pJGfcqNiUAPt0V1VmEomqczOxbajCB0cCVmjwXjWBRKKQIHj1bKSLCx
7hchfhMAVmM407ByAq577Q4BKvQC0MHVkNZp8yrc1ozm23o2KZX2QW19vhyb
Pl0EHZyXk/qnBlPAfMgbPmimVFhwhxwTepDlXRWzTBY/D9UFGR7hedSJLqvs
dDZOMuxLHU3dKB5kvEQkayo6twMPafPH38RYR60F1CzJnbUjl56L0t//A1jf
kk2wsbUwfKFbbaQ+x24NWOUNBFpZJSdD0PtkCtnQzq+fn2Mp6sPx+61jaJXy
OotE2pQ7Rzu8J02qiLmb2tXaVgCJR3wp5mi0AKjp2BzYvMGAwgpc4azBy2xQ
YmABnmLmiNMtzzeWXNfHQW9HcVc3CF+X9zUfyAUSHbKpekjw6v7n0sIcJzUB
eCOzQVm0rA/ZNB+G84nzcKiWc7bEzewKjw7swBngqRhcLY1+AxJ+YDLX7udb
TjOzxytMVrCa/cys3iXxxEMKL1YrGjZLwZMs1G0a6e8WHyC5UJUHjNfu2FPG
yoQGz68FCmzsJ59a8lIYwOXDxL0fq8frTJHFYtOsY2bqbhKhU2UWfXkbgecJ
HBQi5Pw1/d9tn0uHGdUjRNUGJmD5pK+LBVQ7OzppWtqc8WqAlNv53DEwH+1o
efmpW5LWnNhB0X8N3AlLUCbGdD0iBzUDwYlIwPcPfos7fft+hRAiPazAFuzM
z+q5ICoJiWt7J4ug3dXh2+87wD2EQgKamK29avMFp/mehXsM3Q1ujrIt6dd9
9X7RlgRqWc4KECQ8KI4WOvLAKF2q4QeUEuSQA7hCcOEMeBHY7SW83bW3+2D/
J/wA8009dEV04ZTJ8GW/ucGwduR8ZrK1rlfVoWlV02/Cr3ISrnOAhO/748zP
5BSchYZ/IxEq7NQduXeUvFzGT2QIpLbwiJdxXDTyuI0SfpBjpkp6hdVaprqC
46ORAwIbXEkw/FCo3TflFVFQj3QcB30Ul9uFOmxuzCzzd1YluYUReO42eJI4
blbLzIFOHF+R+JIm4SHGOcUwmc8/bReqNX9msiCz4w2mK5WLgCtY+D3LfVP9
Dinzh5ie2znCiF8b7pJnRRmftbtkYyO9YyYBCQLfD2xbSF2rts6tWs7ManaC
Cy9MOQ2x3VbF+exx/Q3BxRyKmme5pWKT0f107Bs0irXjTeHpSIvYa/ASOb3z
9l6tLIm70g+Tdq+F0o/8kbGfNYU+wEaWJEkVNJFbxjNCP8npi3PcUDu8nmnw
G2PQqHDSP+kNZgsKPqkm7vwrNs4ey/NPQoLiFWTQRbt/414yLdAFoWckEzVZ
R9foOzeeT3RRZ7HtVMuiyLe8KEtuFiALvpeXz+UUfRw9i/ycn0FNSGdcVqey
DhXcI1Z+u6IXC9TiuahT/ncGaxdWDxrGyFTHIXk+RumZJSHdTaqm3uieT9KC
vLFoAOpONgBincgHEQtXayiZSGLNNXx4NrkEqvYx9Ix6xwgrEuTLaiPvaKfI
AtvZyKlHde0x0FAe8HYE9f+leA2k5x24DnvvXUz5QAJxXWY/sP+Fu/9hz3WS
y62z67xXUuAlXLzPqUYZeNYXyxkL4OChbqjAA0yJK6oaBN5qvyzma3Ft8HhY
lAoRUTgbOTtw434E+4TXaAGQrSYLjfAVGzPunKRURDaz/iLTP5sUYXsIYy3I
EH51DVyzUv04ATcyeuYRvIph8NoceL8b5u9sqPi0+76XfqoO/7jLakH1byVA
+0xshTCLpstpKI/9HijuPQPRg/PNMSJ0ZWxla2phQHByb3Rvbm1haWwuY29t
IiA8dGVsZWtqYUBwcm90b25tYWlsLmNvbT7CwXUEEAEIACkFAltq/A4GCwkH
CAMCCRD/pWYZp+WrfgQVCAoCAxYCAQIZAQIbAwIeAQAALbgQAJzWwgMiylfq
uL0qNYjVY/K+VmHuS/jlZ5wlDYjbYtlwjQ0WC9hT700YZYsatgPXtE+aZQvM
nelac7fCzOemB0wFtFbKhb7L7Ha7/je9wTQd2rN8oZYAY/HVescbJzOjI2Yw
epHvyH1Kgy/TTB496gkJNw9Jc1my4rP99xOzAej6d6ZOLEETV1XZUtc3QYJl
k+tFx7FuCC8mSAI5TuJ/E1U0A/ykU5bKQuRG5gBrRTmChesEvgTQJoymyx0S
OK1S0j4XXlkvvDYUnndBtHT24f6UOUgZjKZz778QrLGT3fF2/HYZW04Jp6IA
ksBgvPIEA8CS8QUYKQ3jiPz+O1cPflB1QSwYGcI0UWKTwOzM/tVbVFPNjG88
4iEgsPFql6/bj5WSOl3TpmuSOVXz19+IDhP7ndk15U02j+XXnUCqwF9neYwC
Wdq6yBuCNNpbDAOaxguQci8G5vSOBzd5mD09n0EQtppD5jfGK6+hpYEWnOpJ
D3NlfigSFQaD0g6rTcGtOpZzKi0XWB6Pkr0uwRXptH4TEWzir05Q0ypo5Qa5
zfbTUCLGDaZ+ERBPbaOL5aKAFzXLzE7PgTYbjeG3Xs5rqEfsHpy9RaKwXhJr
zgHqSEwv19lZHNwbuvGX4151iLm41GjBSuQ0h/K0QLf61j5q5qpgUP05CdPn
DuhfXPlLeCkax8aGBFtq/A4BEADWk8fFiSMqO0RrNUPT/AunqU0qdtt+fK60
zQQt7CxovMyGRHUhX4CBs8eIE5zJdB8EG4IPXjtO6A7nXbfDWPH4w9CNo4vP
WSsw1gyYxnyzF/DMd7Klsyn7kqdT0vN0TsYISqzNt+3p/51vealBeKM034HO
GcsS/5tiHL6VAnaaF9/rquP/WzreqXO3u6GX+x98GEPW4aw5Y+Sxh9HpTmrj
unv3zfjWJRQueJqouyyHPKfxeIiKcxuMXxPWmQ8wo5uBgNOQoAN6MIHXEr9s
QLJZZR3Pw1waauScdeEpLNLFA/3cqn3SA4A8EJy9NWL/m0h2SBhNdG884MCp
BAA9O6PSd7F0fVidF1nClPs3RWhpQKdAED8Ng8pcJsCHKhiFRnMvTuFc6DYH
8ge2XZFemDVuBe9TOCxi9KpuzhvWGFRh8KW62fNFJcq7M/Wdj66OzFwkuk7V
0yTqdML64h17mHMf1XMn4kXb1iab8fM3O67Da48/9qsl5/0yeIYs+BqRtHxA
ptLJe18pP5iM8V4wbqoxV5uw3AQUhswdHiMwohLwAgLy6Iy6BYezX65hX18q
4S7isXoWU2cTtB+DFKorHU22PN32msYCI3G2eHqsVu/0r3d55zML2azaq9lK
MOco4IUdTSWxWsNz6liaMXh1bQS+gczmEb9/TAvOAVUFn8dizwARAQAB/gkD
CBDHKiHE2arlYFI3efGw8KoyU2WcFfYw8/z0kQVCWfB+vWJsBo4CvR/AvHKC
lv+s1XFzFv+51iCiecM/779TI1B/AJ3OEOL8331gZ8aCfTEKsGrJTL4mBOao
g2fUpGc0zOs0iRTH1jpQyOfZrr1Hr54PAvpjDKyBVxTvDuzQ3/qRj0AiOJiy
Oecq877kgdQmTUtZLGk3MwwerYiNkcoxNGiZh5I71mwTYeV6R+LlJj5pgIvj
k8lvDdDBVt9L50uqHx+saDJAkJzzx0QWPcY1IeOV/w6DpvrPFBQZna6VQy2e
u83+ROsbIAgAY7JDvuVVad2skBCFu9FdJuha3G1+IVQ+NwVEKNFEAdNDQ3L5
Jg+7io0fGINq5oq/ssQhXNeVhJ1sQKbBy/y3+X+3OCTT/MgDeuURhHhIgijq
Gz/KMOxKsq/yeGpV025Hqh/w8BK3KG87/yB6M+gnCY1jWo4OuJ0Js4rzz7oq
lVREd+fCGDGK3FJNvGIZ41jrnhYmmywlJg4wpjp/WVWfpznL+iqAEaxlPoZz
R5gakPyQ3/c17npvI0QCW6fc/5GSg7cKJ6sNxWuYzSsOfnR9ZwZZIkE0NYTW
fCXibcNl3+8Kf8KXgRmOQ/nKNkGGmHQdTAAgA/UUSxguGYx5x622+dvlPuLG
7cxTx3ghTGL3+DC0uZuN59zd4RumFH28uAk5W7kG/SKMOiMwIN6whKtW+ltv
Hzl5h41Yx2U60G3Gkh1f2wCztJbxfjtXitksPxxBsjKE+u0beAA3LkWQFZum
mcPEM69ZGn+QRLuHyQhWY/q3LntFVhSggR+WYgrhsdFSpO1Rbj2Ly8GW2z75
U+fL8b0HYxbKfotI7+ICw5u9GPs8rsMWtwehqpxTrzNmOOHKw1v0xVzUOsvw
7ODBmS+GTCJmOKQOuUR1TLlLv/o16kYef9B6ga3wPkibMVwEA/5vemAAK+Sc
IBmS/9yg4Kf6fVRhe1UAkLj053IKDCiT/AbjtpeikaPGwchXLY49AxbxqVAQ
7UA+Rr/MOCebpFadtgZ8DoIiGufxOZpnqQ8FbtBQcS+2zplAHEls94ihfSDB
5IvPOO3UW3/OPaCov1bm/VDLiJH03WRUAS4nZV/y9BbwsYP18s1ejC7BCuAZ
9gswM13R8LNsVei/vdkq6GFwMTRvQHW9tMip63uFbfo0KnztQeSEMFP8WKRi
gODTMVXSl+OzIiFuSlbTEeY6BIRYy/Shivupp58dQqb+X9VPn8iUfLFoJA4d
ogFydMr4Qp8upEYjtadJuz0a4berrfm8gUQRnaG5eYxhwbyhL40r08dRYbKG
7NXI/30KfmsKIo4V6Wf224v218+BleFAl4DY8/jMwOy0nFRcWE5Wl9w3+J2V
Bn0Rq5iJlR3owmgNheW/TNLBmRAaqk85voXqMr0sJfFUjFfMr16oIOHRWWu9
UOgEKRwWD+//7EQQmIgII6HeBGJ2UgQHQCfiYJetRLm+7pzqbp4QeTM+OQhN
oKLL6AoddPiRHjoV3C1uWfSfvo8oyPocAIUuZDKK2AgeORX/L4A5d6Hue2yl
vUmCIfx8HaRLTMH0GWA31yKjuwj18J+O7rTNWakgxXQQk0AhpCTpnL90sCex
5hSf+Xxl6Vozv20ltOrh/5ZLUbJp1xM3TdEFBjkysMMWG0wZshEt7cuQTrVY
R4piWG7OMXnMOr1sSS59j5RDmDzsOJo7soXQcsY5SfIQnxe1Ec8FTz2l3uvW
U8xeBPsnsDXTT/snscayBisvhacE6F9bOYgMG3jCwV8EGAEIABMFAltq/A4J
EP+lZhmn5at+AhsMAAAncg/8CsMbIzl4GjDpkvul0ULJhJC2p7SWj0G1a1EP
xnCV41s7QcrPCCInTk4nDmQeJmrJqx2TTE9awJ2q26yZTQu9yrCh1Sn/ciqz
jnRF3SdRlWkTvcm9EwBnRR7AAFdiBxT+gLrQg2dHDaH5E3Wci0GboiXzoziN
UOo1YQAjpeSn5AnHp1q9vZPBR6JSsnEHs3CyA2vMjdbfJuU9MoPjbtKli20N
3VvCuKLWsvvVwz5Hal724EXPmYayvD6aSyY9gMsAJkCQyZhSeD5rZQVib8+j
9aqZVaDZvpWT2xvwmwgzeUfFFbkzvCLjhyjV2swAYXM3l4GOiqejbssJMwaz
vMzTXpUShOgv22aJD08WHwKFLgmNljeOhdJmr3ss0bT1rsppE95B673FfrQZ
7CJZN7E+whuwZv49j8qZp0lkYFFVzxud2TZ4UxLqstDJVD95ZJ4h2vPUaWzc
1rx6luQA+laacONTSFXcUd58/Z+2m59Z57jb3WwnjEnd7ST2zsHFEH4Wlztv
sqa8rTTMTow22ZHuITxw7odPCZ8+Li2sRUTGGpR8u0Y4uiALDYtqk0F27R6s
z9GxJikRwscymWmXx2QsvhUiWeOJ05WwK+WAnKR1uVtkEJ9QJVe2chyuMORY
+GqpPgyAFE55MktNrnSCi3wuXvh6XoQhQ5BOshZuQb9BMY0=
=xWqS
-----END PGP PRIVATE KEY BLOCK-----";

const TEST_VERIFICATION_KEY_GO: &str = "-----BEGIN PGP PUBLIC KEY BLOCK-----

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
OQTmvM6R
=DvFv
-----END PGP PUBLIC KEY BLOCK-----
";

const TEST_EXPECTED_BODY_MIME_GO: &str = r"Pak

> On Aug 13, 2018, at 4:15 PM, Telekja <telekja@protonmail.com> wrote:
> 
> Encrypted & Signed PGP part
> ssdaadsdasads
> 
> 
> Sent with ProtonMail Secure Email.
> 
> ‐‐‐‐‐‐‐ Original Message ‐‐‐‐‐‐‐
> On August 13, 2018 4:12 PM, PGP Test <pmpgptest@gmail.com> wrote:
> 
>> Test 123
>> BOBOHRI
> 
> 

";

struct TestMessage(pub bool, pub String);

impl GettablePGPMessage for TestMessage {
    fn pgp_message(&self) -> &[u8] {
        self.1.as_bytes()
    }
}

impl DecryptableMessage for TestMessage {
    fn message_is_mime(&self) -> bool {
        self.0
    }

    fn message_id(&self) -> Option<&str> {
        Some("unique-message-id")
    }
}

#[test]
fn test_message_decrypt_and_verify() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_keys(&pgp);
    let mut verification_keys = get_test_public_address_keys(&pgp);
    let test_message = TestMessage(false, TEST_MESSAGE_BODY.into());
    let decrypted_message = test_message.decrypt(&pgp, &decryption_keys).unwrap();
    let decrypted_message_body = decrypted_message.processed_body().unwrap();
    assert_eq!(decrypted_message_body.body(), TEST_EXPECTED_BODY);
    let verification_result = decrypted_message.verify_signature(&pgp, &verification_keys);
    assert!(verification_result.is_ok());
    verification_keys.remove(0);
    let verification_result_no_verifier =
        decrypted_message.verify_signature(&pgp, &verification_keys);
    assert!(matches!(
        verification_result_no_verifier.unwrap_err(),
        VerificationError::NoVerifier(_)
    ));
}

#[test]
fn test_message_decrypt_and_verify_mime() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_key_source(&pgp, TEST_DECRYPTION_KEY_MIME, "password");
    let verification_keys = get_test_public_address_key_source(&pgp, TEST_VERIFICATION_KEY_MIME);

    let test_message = TestMessage(true, TEST_MESSAGE_MIME.into());
    let decrypted_message = test_message.decrypt(&pgp, &decryption_keys).unwrap();
    let decrypted_message_body = decrypted_message.processed_body().unwrap();
    assert_eq!(decrypted_message_body.body(), TEST_EXPECTED_BODY_MIME);
    let verification_result = decrypted_message.verify_signature(&pgp, &verification_keys);
    assert!(verification_result.is_ok());

    assert!(decrypted_message_body.is_mime());
    let DecryptedBody::Mime(processed_messsage) = decrypted_message_body else {
        panic!("Must be a mime body");
    };

    assert_eq!(processed_messsage.encrypted_subject.unwrap(), "test mime");

    assert_eq!(processed_messsage.attachments.len(), 4);
    for (idx, attachment) in processed_messsage.attachments.iter().enumerate() {
        if idx != processed_messsage.attachments.len() - 1 {
            let expected_content = format!("attachment{}", idx + 1);
            let expected_name = format!("{expected_content}.txt");
            assert_eq!(attachment.name, expected_name);
            assert_eq!(
                String::from_utf8(attachment.data.clone()).unwrap(),
                expected_content
            );
        }
    }

    let last_attachment = processed_messsage.attachments.last().unwrap();
    assert_eq!(last_attachment.name, "OpenPGP_0x46F0FA708D336220.asc");
}

#[test]
fn test_message_decrypt_and_verify_mime_go() {
    let pgp = proton_crypto_inbox::proton_crypto::new_pgp_provider();
    let decryption_keys = get_test_address_key_source(&pgp, TEST_DECRYPTION_KEY_GO, "test");
    let verification_keys = get_test_public_address_key_source(&pgp, TEST_VERIFICATION_KEY_GO);

    let test_message = TestMessage(true, TEST_MESSAGE_BODY_GO.into());
    let decrypted_message = test_message.decrypt(&pgp, &decryption_keys).unwrap();
    let decrypted_message_body = decrypted_message.processed_body().unwrap();
    assert_eq!(decrypted_message_body.body(), TEST_EXPECTED_BODY_MIME_GO);
    let verification_result = decrypted_message.verify_signature(&pgp, &verification_keys);
    assert!(verification_result.is_ok());

    assert!(decrypted_message_body.is_mime());
    let DecryptedBody::Mime(processed_messsage) = decrypted_message_body else {
        panic!("Must be a mime body");
    };

    assert!(processed_messsage.attachments.is_empty());
    assert!(matches!(
        processed_messsage.mime_body_type,
        ProcessedBodyType::Text
    ));
    assert!(processed_messsage.encrypted_subject.is_none());
}
