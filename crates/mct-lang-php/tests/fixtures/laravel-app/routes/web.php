<?php

use App\Http\Controllers\HomeController;
use App\Http\Controllers\OrderController;
use App\Http\Controllers\PhotoController;
use App\Http\Controllers\ReportController;
use Illuminate\Support\Facades\Route;

Route::get('/', HomeController::class);
Route::view('/welcome', 'welcome');
Route::get('/orders', [OrderController::class, 'index']);
Route::post('/orders', [OrderController::class, 'store'])->name('orders.store');
Route::resource('photos', PhotoController::class)->only(['index', 'show']);
Route::controller(ReportController::class)->group(function () {
    Route::get('/reports/{id}', 'show');
    Route::get('/reports', 'index');
});
Route::get('/legacy', 'LegacyController@index');
