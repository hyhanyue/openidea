package com.example.runtime;

import com.example.runtime.base.BaseController;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RestController;

@RestController
@RequestMapping("/runtime-child")
public class ChildController extends BaseController {
}
