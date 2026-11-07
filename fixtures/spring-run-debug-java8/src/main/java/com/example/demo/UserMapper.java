package com.example.demo;

import org.apache.ibatis.annotations.Mapper;
import org.apache.ibatis.annotations.Param;
import java.util.List;

@Mapper
public interface UserMapper {
    User findById(@Param("id") Long id);

    List<User> findByName(@Param("name") String name);
    long countByName(@Param("name") String name);
}
