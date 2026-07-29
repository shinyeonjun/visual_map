class BaseEntity
  attr_reader :id

  def initialize(id)
    @id = id
  end
end

class User < BaseEntity
end

class Box
  def initialize(value)
    @value = value
  end

  def get
    @value
  end
end
