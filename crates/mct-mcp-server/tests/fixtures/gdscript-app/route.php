<?php
use Symfony\Component\Routing\Attribute\Route;

class WorkshopController {
    #[Route('/api/workshop', methods: ['GET'])]
    public function workshop() {
        return 'Workshop materials available';
    }
}
