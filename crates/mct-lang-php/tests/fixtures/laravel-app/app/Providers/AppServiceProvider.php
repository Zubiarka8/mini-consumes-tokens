<?php

namespace App\Providers;

use App\Events\OrderPlaced;
use App\Listeners\AuditOrder;
use Illuminate\Support\Facades\Event;
use Illuminate\Support\ServiceProvider;

class AppServiceProvider extends ServiceProvider
{
    public function register(): void
    {
    }

    public function boot(): void
    {
        Event::listen(OrderPlaced::class, AuditOrder::class);
    }
}
