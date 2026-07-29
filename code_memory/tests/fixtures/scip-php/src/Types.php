<?php

namespace VisualMap;

interface Entity
{
    public function id(): string;
}

final class User implements Entity
{
    public function __construct(private string $value) {}

    public function id(): string
    {
        return $this->value;
    }
}

/** @template T of Entity */
final class Box
{
    /** @param T $value */
    public function __construct(private Entity $value) {}

    /** @return T */
    public function get(): Entity
    {
        return $this->value;
    }
}
