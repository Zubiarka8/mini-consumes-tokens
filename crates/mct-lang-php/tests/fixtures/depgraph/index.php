<?php

use App\Models\User;
use App\Models\{Post, Tag as Label};
use function App\helpers\format_name;

require_once __DIR__ . '/lib/helpers.php';
require 'lib/boot.php';
include $computed;

echo format_name(new User(), new Post(), new Label());
