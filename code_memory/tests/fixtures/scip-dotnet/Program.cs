namespace VisualMapFixture;

public class Program : BaseProgram, IRunner
{
    public static int Add(int left, int right) => left + right;

    public static void Main()
    {
        var box = new Box<string>("user-1");
        Add(1, 2 + box.Get().Length);
    }

    public int Run()
    {
        return BaseValue();
    }
}
