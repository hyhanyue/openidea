package com.example.demo;

import org.springframework.stereotype.Service;
import java.util.List;

@Service
public class UserService {
    private final UserMapper userMapper;

    public UserService(UserMapper userMapper) {
        this.userMapper = userMapper;
    }

    public User findById(Long id) {
        return userMapper.findById(id);
    }

    public List<User> findByName(String name) {
        return userMapper.findByName(name);
    }
}
