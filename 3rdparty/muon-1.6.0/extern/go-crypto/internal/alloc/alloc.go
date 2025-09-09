package alloc

import "sync"

// ObjMap holds a map of objects of any type.
type ObjMap[T any] struct {
	// The items in the map.
	items map[int]*T

	// The next index to allocate.
	index int

	// The mutex for the map.
	mutex sync.RWMutex
}

// Creates a new object map.
func New[T any]() *ObjMap[T] {
	return &ObjMap[T]{
		items: make(map[int]*T),
	}
}

// Alloc allocates a new object, returning its index.
func (m *ObjMap[T]) Alloc(val *T) int {
	m.mutex.Lock()
	defer m.mutex.Unlock()

	index := m.index
	m.items[index] = val
	m.index++

	return index
}

// Deref returns the object at the given index.
func (m *ObjMap[T]) Deref(index int) *T {
	m.mutex.RLock()
	defer m.mutex.RUnlock()

	return m.items[index]
}

// Take takes the object at the given index;
// it will no longer be managed by the map.
func (m *ObjMap[T]) Take(index int) *T {
	m.mutex.Lock()
	defer m.mutex.Unlock()

	val := m.items[index]

	delete(m.items, index)

	return val
}

// Return how many items are in the map.
func (m *ObjMap[T]) Len() int {
	m.mutex.RLock()
	defer m.mutex.RUnlock()

	return len(m.items)
}
