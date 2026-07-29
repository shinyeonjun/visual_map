<?php

require_once __DIR__ . '/src/Types.php';

use VisualMap\Box;
use VisualMap\User;

function add(int $left, int $right): int
{
    return $left + $right;
}

function run(): void
{
    $box = new Box(new User('user-1'));
    add(1, strlen($box->get()->id()));
}
