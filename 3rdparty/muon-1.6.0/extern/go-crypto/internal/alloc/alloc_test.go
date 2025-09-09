package alloc

import (
	"testing"
)

func TestAlloc(t *testing.T) {
	m := New[int]()

	// Allocate two ints.
	foo := m.Alloc(new(int))
	bar := m.Alloc(new(int))

	// Both should have initial values `0`.
	if *m.Deref(foo) != 0 {
		t.Errorf("expected 0, got %d", *m.Deref(foo))
	}

	if *m.Deref(bar) != 0 {
		t.Errorf("expected 0, got %d", *m.Deref(bar))
	}

	// Set the values to something else.
	*m.Deref(foo) = 1
	*m.Deref(bar) = 2

	if *m.Deref(foo) != 1 {
		t.Errorf("expected 1, got %d", *m.Deref(foo))
	}

	if *m.Deref(bar) != 2 {
		t.Errorf("expected 2, got %d", *m.Deref(bar))
	}

	// Take the values.
	if *m.Take(foo) != 1 {
		t.Errorf("expected 1, got %d", *m.Take(foo))
	}

	if *m.Take(bar) != 2 {
		t.Errorf("expected 2, got %d", *m.Take(bar))
	}

	// Allocator should be empty.
	if m.Len() != 0 {
		t.Errorf("expected 0, got %d", len(m.items))
	}
}
