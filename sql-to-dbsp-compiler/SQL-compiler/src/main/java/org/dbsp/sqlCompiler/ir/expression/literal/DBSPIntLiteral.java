package org.dbsp.sqlCompiler.ir.expression.literal;

import org.dbsp.sqlCompiler.compiler.frontend.CalciteObject;
import org.dbsp.sqlCompiler.ir.type.DBSPType;
import org.dbsp.sqlCompiler.ir.type.primitive.DBSPTypeInteger;

import java.util.Objects;

/**
 * Base class for all integer literal.
 */
public abstract class DBSPIntLiteral extends DBSPLiteral {
    protected DBSPIntLiteral(CalciteObject node, DBSPType type, boolean isNull) {
        super(node, type, isNull);
    }

    public DBSPTypeInteger getIntegerType() {
        return this.type.to(DBSPTypeInteger.class);
    }
}
