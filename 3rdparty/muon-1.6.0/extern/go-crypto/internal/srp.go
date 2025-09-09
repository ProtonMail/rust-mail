package internal

import (
	"github.com/ProtonMail/go-srp"
)

// NewVerifier creates a new SRP verifier for the given password.
func NewVerifier(pass, salt []byte) (int, []byte, []byte, error) {
	auth, err := srp.NewAuthForVerifier(pass, modArm, salt)
	if err != nil {
		return 0, nil, nil, err
	}

	verifier, err := auth.GenerateVerifier(modLen)
	if err != nil {
		return 0, nil, nil, err
	}

	return auth.Version, auth.HashedPassword, verifier, nil
}
