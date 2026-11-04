@extends('layouts.app')
@inject('metrics', 'App\Services\Metrics')
@section('content')
    @include('orders._row', ['row' => $row])
    @includeWhen($show, 'orders.missing')
    @include($dynamic)
    <x-badge />
    <x-alert />
    <x-vendor::thing />
@endsection
