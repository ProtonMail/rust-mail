package internal

import (
	"go-crypto/internal/alloc"

	"github.com/ProtonMail/go-srp"
)

// Ongoing SRP challenges stored in a global allocator.
// These can be allocated via `alloc.Alloc(...)` and freed via `alloc.Take(...)`.
var servers = alloc.New[srp.Server]()

// Create a new SRP challenge.
func NewChallenge(verifier []byte) (int, []byte, string, error) {
	server, err := srp.NewServerFromSigned(modArm, verifier, modLen)
	if err != nil {
		return 0, nil, "", err
	}

	challenge, err := server.GenerateChallenge()
	if err != nil {
		return 0, nil, "", err
	}

	return servers.Alloc(server), challenge, modArm, nil
}

// Verify a client's proof.
func VerifyProof(id int, ephem, proof []byte) ([]byte, error) {
	return servers.Take(id).VerifyProofs(ephem, proof)
}
