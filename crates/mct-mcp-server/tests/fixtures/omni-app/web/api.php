<?php

require_once 'repository.php';

function handle_request($account)
{
    return load_rows($account);
}

function describe_request($account)
{
    $rows = handle_request($account);
    return count($rows);
}
