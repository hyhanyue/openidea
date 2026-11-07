package com.example.runtime;

import com.example.runtime.base.BaseController;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/runtime-override")
public class OverrideController extends BaseController {
    @Override
    @GetMapping("/override")
    public String inherited() { return "overridden fixture"; }
}
