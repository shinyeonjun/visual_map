namespace TypeRelationFixture;

public sealed class Payload {}

public sealed class ResultValue {}

public class BaseService
{
    public virtual ResultValue Execute(Payload input) => new();
}

public interface IContract
{
    ResultValue Execute(Payload input);
}

public interface IParentContract {}

public interface IChildContract : IParentContract {}

public sealed class Service : BaseService, IContract
{
    private Payload current;

    public Service(Payload current)
    {
        this.current = current;
    }

    public override ResultValue Execute(Payload input)
    {
        Payload transient = input;
        current = transient;
        return new ResultValue();
    }
}
