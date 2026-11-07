package com.example.demo;

import org.springframework.web.bind.annotation.GetMapping;
import org.springframework.web.bind.annotation.PathVariable;
import org.springframework.web.bind.annotation.RequestMapping;
import org.springframework.web.bind.annotation.RequestParam;
import org.springframework.web.bind.annotation.RestController;
import org.springframework.web.server.ResponseStatusException;
import org.springframework.http.HttpStatus;
import java.util.List;

@RestController
@RequestMapping("/users")
public class UserController {
    private final UserService userService;
    private final DemoProperties properties;

    public UserController(UserService userService, DemoProperties properties) {
        this.userService = userService;
        this.properties = properties;
    }

    @GetMapping("/{id}")
    public User findById(@PathVariable Long id) {
        User user = userService.findById(id);
        if (user == null) {
            throw new ResponseStatusException(HttpStatus.NOT_FOUND, "User not found");
        }
        return user;
    }

    @GetMapping
    public List<User> findByName(@RequestParam(defaultValue = "") String name) {
        return userService.findByName(name);
    }

    @GetMapping("/greeting")
    public String greeting() {
        return properties.getGreeting();
    }
}
