package main

/*
#include <stdlib.h>
#include <stdint.h>

typedef struct { uint8_t *ptr; size_t len; } Raw;
*/
import "C"

import (
	"unsafe"

	"go-crypto/internal"

	"github.com/ProtonMail/go-srp"
	"github.com/ProtonMail/gopenpgp/v2/crypto"
)

//export Result
type Result int8

//export Ok
func Ok() Result {
	return Result(0)
}

//export Err
func Err() Result {
	return Result(1)
}

//export getMboxPass
func getMboxPass(pass, salt []byte) (C.Raw, Result) {
	hash, err := srp.MailboxPassword(pass, salt)
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(hash[len(hash)-31:]), Ok()
}

//export newRawPgpKey
func newRawPgpKey(name, addr, pass []byte) (C.Raw, Result) {
	obj, err := crypto.GenerateKey(string(name), string(addr), "x25519", -1)
	if err != nil {
		return makeRaw(nil), Err()
	}

	return lockPgpKey(obj, pass)
}

//export toArmPgpKey
func toArmPgpKey(rawKey []byte) (C.Raw, Result) {
	obj, err := crypto.NewKey(rawKey)
	if err != nil {
		return makeRaw(nil), Err()
	}

	arm, err := obj.Armor()
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw([]byte(arm)), Ok()
}

//export toRawPgpKey
func toRawPgpKey(armKey []byte) (C.Raw, Result) {
	obj, err := crypto.NewKeyFromArmored(string(armKey))
	if err != nil {
		return makeRaw(nil), Err()
	}

	raw, err := obj.Serialize()
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(raw), Ok()
}

//export lockRawPgpKey
func lockRawPgpKey(key, pass []byte) (C.Raw, Result) {
	obj, err := crypto.NewKey(key)
	if err != nil {
		return makeRaw(nil), Err()
	}

	return lockPgpKey(obj, pass)
}

//export lockArmPgpKey
func lockArmPgpKey(key, pass []byte) (C.Raw, Result) {
	obj, err := crypto.NewKeyFromArmored(string(key))
	if err != nil {
		return makeRaw(nil), Err()
	}

	return lockPgpKey(obj, pass)
}

func lockPgpKey(key *crypto.Key, pass []byte) (C.Raw, Result) {
	enc, err := key.Lock(pass)
	if err != nil {
		return makeRaw(nil), Err()
	}

	raw, err := enc.Serialize()
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(raw), Ok()
}

//export unlockRawPgpKey
func unlockRawPgpKey(key, pass []byte) (C.Raw, Result) {
	obj, err := crypto.NewKey(key)
	if err != nil {
		return makeRaw(nil), Err()
	}

	return unlockPgpKey(obj, pass)
}

//export unlockArmPgpKey
func unlockArmPgpKey(key, pass []byte) (C.Raw, Result) {
	obj, err := crypto.NewKeyFromArmored(string(key))
	if err != nil {
		return makeRaw(nil), Err()
	}

	return unlockPgpKey(obj, pass)
}

func unlockPgpKey(key *crypto.Key, pass []byte) (C.Raw, Result) {
	dec, err := key.Unlock(pass)
	if err != nil {
		return makeRaw(nil), Err()
	}

	raw, err := dec.Serialize()
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(raw), Ok()
}

//export decryptRawMsg
func decryptRawMsg(msg []byte, keys [][]byte) (C.Raw, Result) {
	return decryptMsg(crypto.NewPGPMessage(msg), keys)
}

//export decryptSplitMsg
func decryptSplitMsg(pkts, data []byte, keys [][]byte) (C.Raw, Result) {
	return decryptMsg(crypto.NewPGPSplitMessage(pkts, data).GetPGPMessage(), keys)
}

//export decryptArmMsg
func decryptArmMsg(msg []byte, keys [][]byte) (C.Raw, Result) {
	obj, err := crypto.NewPGPMessageFromArmored(string(msg))
	if err != nil {
		return makeRaw(nil), Err()
	}

	return decryptMsg(obj, keys)
}

func decryptMsg(msg *crypto.PGPMessage, keys [][]byte) (C.Raw, Result) {
	kr, err := crypto.NewKeyRing(nil)
	if err != nil {
		return makeRaw(nil), Err()
	}

	for _, key := range keys {
		if key, err := crypto.NewKey(key); err != nil {
			return makeRaw(nil), Err()
		} else if err := kr.AddKey(key); err != nil {
			return makeRaw(nil), Err()
		}
	}

	dec, err := kr.Decrypt(msg, nil, crypto.GetUnixTime())
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(dec.GetBinary()), Ok()
}

//export newSrpSalt
func newSrpSalt() C.Raw {
	salt, err := crypto.RandomToken(10)
	if err != nil {
		panic(err)
	}

	return makeRaw(salt)
}

//export newSrpVerifier
func newSrpVerifier(pass, salt []byte) (uint8, C.Raw, C.Raw, Result) {
	version, passHash, verifier, err := internal.NewVerifier(pass, salt)
	if err != nil {
		return 0, makeRaw(nil), makeRaw(nil), Err()
	}

	return uint8(version), makeRaw(passHash), makeRaw(verifier), Ok()
}

//export newSrpChallenge
func newSrpChallenge(verifier []byte) (uint64, C.Raw, C.Raw, Result) {
	id, challenge, modulus, err := internal.NewChallenge(verifier)
	if err != nil {
		return 0, makeRaw(nil), makeRaw(nil), Err()
	}

	return uint64(id), makeRaw(challenge), makeRaw([]byte(modulus)), Ok()
}

//export verifySrpProof
func verifySrpProof(id C.uint64_t, ephem, proof []byte) (C.Raw, Result) {
	serverProof, err := internal.VerifyProof(int(id), ephem, proof)
	if err != nil {
		return makeRaw(nil), Err()
	}

	return makeRaw(serverProof), Ok()
}

//export freeRaw
func freeRaw(data C.Raw) {
	C.free(unsafe.Pointer(data.ptr))
}

func makeRaw(b []byte) C.Raw {
	buf := C.CBytes(b)
	ptr := (*C.uchar)(buf)
	len := C.size_t(len(b))

	return C.Raw{ptr, len}
}

func main() {}
