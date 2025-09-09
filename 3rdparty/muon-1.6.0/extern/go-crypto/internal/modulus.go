package internal

import (
	_ "embed"

	"go-crypto/internal/util"

	"github.com/ProtonMail/gopenpgp/v2/crypto"
)

//go:embed modulus.asc
var asc []byte

//go:embed modulus.sig
var sig []byte

// The signed, armored modulus used for SRP.
var modArm = util.Must(crypto.NewClearTextMessage(asc, sig).GetArmored())

// The bit length of the modulus.
var modLen = 2048
