using Xunit;

namespace TestRelations;

public sealed class CalculatorTests
{
    [Fact]
    public void DoublesAValue()
    {
        Calculator.Calculate(2);
    }

    [Fact]
    public void NameOnlyCalculate() {}
}
