package com.example.runtime.base;

import org.springframework.web.bind.annotation.GetMapping;

public class BaseController {
    @GetMapping("/inherited")
    public String inherited() { return "inherited fixture"; }
}
