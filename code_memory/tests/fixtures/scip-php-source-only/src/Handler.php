<?php

namespace App;

final class Handler
{
    public function handle(Service $service): int
    {
        return $service->save();
    }
}
