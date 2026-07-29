import 'types.dart';

int add(int left, int right) => left + right;

void main() {
  final user = User('user-1');
  add(1, user.id.length);
  Box<User>(user).get();
}
