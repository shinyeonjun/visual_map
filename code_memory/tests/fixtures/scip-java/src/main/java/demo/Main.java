package demo;

public class Main extends Base implements Runner {
    public static int add(int left, int right) {
        return left + right;
    }

    public static void main(String[] args) {
        Box<String> box = new Box<>("user-1");
        add(1, 2 + box.get().length());
    }

    @Override
    public int run() {
        return baseValue();
    }
}
