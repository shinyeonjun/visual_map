namespace VisualMapFixture;

public sealed class Box<T>
{
    private readonly T value;

    public Box(T value) => this.value = value;

    public T Get() => value;
}
