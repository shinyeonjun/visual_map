package typerelations

type Payload struct{}

type ResultValue struct{}

type Contract interface {
	Execute(Payload) ResultValue
}

type Service struct {
	Current Payload
}

func (service Service) Execute(input Payload) ResultValue {
	var transient Payload = input
	service.Current = transient
	return ResultValue{}
}

func run() {
	local := Payload{}
	service := Service{Current: local}
	service.Execute(local)
}
