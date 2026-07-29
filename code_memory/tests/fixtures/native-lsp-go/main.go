package main

type Entity interface {
	ID() string
}

type User struct {
	IDValue string
}

func (u User) ID() string { return u.IDValue }

type Box[T Entity] struct {
	Value T
}

func (b Box[T]) Get() T { return b.Value }

func Add(a, b int) int {
	return a + b
}

func main() {
	box := Box[User]{Value: User{IDValue: "user-1"}}
	_ = Add(1, len(box.Get().ID()))
}
