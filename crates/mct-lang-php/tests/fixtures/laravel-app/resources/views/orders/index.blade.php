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

@use('App\Models\Customer', 'Buyer')
<a href="{{ route('billing.export') }}">{{ __('billing.summary') }}</a>
@php
    $model = Buyer::class;
    $format = config('billing.exports.format');
@endphp
@{{ route('escaped.blade.route') }}
@verbatim
    {{ route('verbatim.blade.route') }}
@endverbatim
@include('billing::receipt')
