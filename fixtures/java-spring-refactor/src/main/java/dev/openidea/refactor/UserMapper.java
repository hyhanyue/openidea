package dev.openidea.refactor;
import org.apache.ibatis.annotations.Mapper;
import org.apache.ibatis.annotations.Param;
@Mapper
public interface UserMapper {
    Integer find(@Param("id") int id);
    Holder nested();
}
