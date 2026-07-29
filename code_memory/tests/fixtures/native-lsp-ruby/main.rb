require_relative "types"

def add(left, right)
  left + right
end

user = Box.new(User.new("user-1")).get
add(1, user.id.length)
