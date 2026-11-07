package com.example.runtime;

import org.springframework.stereotype.Component;
import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

public class NestedFixture {
    @RestController
    @RequestMapping("/runtime-nested")
    public static class InnerController extends com.example.runtime.base.BaseController {
        @GetMapping("/mapping")
        public String nested() { return "nested fixture"; }
    }

    @Component
    public static class InnerBean {
    }
}
