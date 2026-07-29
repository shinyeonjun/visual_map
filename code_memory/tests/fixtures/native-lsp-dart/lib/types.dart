abstract class Entity {
  String get id;
}

class User implements Entity {
  @override
  final String id;

  User(this.id);
}

class Box<T extends Entity> {
  final T value;

  Box(this.value);

  T get() => value;
}
